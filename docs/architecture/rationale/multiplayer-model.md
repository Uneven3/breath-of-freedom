# Rationale: multiplayer host-authoritative, sin servidor dedicado

## El modelo pedido

No es un MMORPG con servidor central: un cliente **hostea** una sesión
(su propia instancia del juego) e invita a N amigos a conectarse — el
equivalente moderno a un LAN party vía Hamachi. No hay proceso "servidor"
separado que alguien despliegue.

## La decisión: host-autoritativo, no P2P, no rollback

El cliente que hostea corre la simulación completa (Movement, Combate,
Monturas, World) como única fuente de verdad; los demás clientes son
"tontos" en el sentido de simulación: envían input, reciben estado.

- **Por qué no P2P con estado compartido**: N copias de la simulación
  divergiendo (float drift, orden de eventos) sin autoridad que desempate es
  la clase de bug más cara de depurar con un equipo de agentes de IA
  desconectados entre sí trabajando en paralelo. Un solo dueño de la verdad
  es más simple de razonar y de testear.
- **Por qué no rollback netcode**: rollback resuelve *inputs de alta
  precisión con ventanas de frames muy angostas* (fighting games). BOTW-like
  es lento y con peso (Pilar de Combate) — la sensibilidad al input exacto
  del frame es baja. Rollback es complejidad que este juego no necesita
  comprar.
- **Por qué host-autoritativo sí encaja**: es el mismo modelo mental que
  cliente-servidor clásico, solo que el "servidor" resulta ser uno de los
  jugadores. Todo lo que ya se diseñó para multi-actor aplica: un actor
  remoto es un `Actor` más, enlazado a un `InputSource` de red en vez de una
  fuente local. (codex)

## Cómo generaliza el patrón Brain

Input ya es la única frontera por donde entra hardware o red a acciones
resueltas (`docs/architecture/input.md`). Un jugador remoto es, para
Movement/Combat/NPCs, un `Actor` (ver `rationale/multi-actor-dispatch.md`)
cuyo `InputControlledBy(InputSource)` apunta a una fuente alimentada por la
red. El host recibe `LocalInputFrame` con un `input::ActionFrame` y
`frame_seq` monotónico; Multiplayer emite `ApplyRemoteActionsMessage`; Input
valida fuente/secuencia y aplica ese frame a su propio `ActiveActions`; y los mismos Brains genéricos traducen esas acciones a
`Intents`/`CombatIntents`/`InteractIntents`. No existe un `NetworkBrain`
separado que duplique la traducción de gameplay. (codex)

En la máquina que hostea: todos los actores (local + remotos) corren
Movement/Combate/Monturas normalmente. En una máquina cliente: su propio
input local se envía a la red en vez de aplicarse directo (no simula nada
en `FixedUpdate`), y el estado replicado desde el host se usa para
interpolar transforms en `Update` — **presentación, no simulación**
(Constitución §20). Un cliente nunca decide su propio `LocomotionState`, el
host lo hace.

## Decisiones abiertas

- Librería de networking: requiere aprobación explícita (Constitución §17).
  Necesita, como mínimo: descubrimiento/invitación de sesión, NAT traversal
  (el caso Hamachi-moderno es sesiones entre IPs no necesariamente en la
  misma LAN), canales confiables (unirse/salir) y no confiables (snapshots
  de estado por frame).
- Sin client-side prediction en la primera versión: los clientes ven el
  mundo con el delay del RTT hacia el host, suavizado por interpolación.
  Aceptable para el ritmo contemplativo del juego (Pilar: Exploración
  contemplativa); se reconsidera si el feeling de Combate lo exige.
- Qué pasa si el host se desconecta: la sesión termina (sin migración de
  host) para la v1.
- Número objetivo de jugadores simultáneos (GDD §13).
- Cómo se une un jugador a mitad de sesión (snapshot completo del estado del
  mundo al conectar).
