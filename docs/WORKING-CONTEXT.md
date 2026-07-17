# Working Context

Este archivo preserva la intención de implementación activa entre sesiones de
agentes. Complementa (no reemplaza) los tickets de `docs/tickets/` y la
arquitectura de `docs/architecture/`. El historial de checkpoints antiguos
vive en git — este archivo describe solo el presente.

## Protocolo

- Léelo antes de continuar el trabajo activo; actualízalo tras cada decisión
  de diseño aceptada, checkpoint de implementación o playtest del usuario.
- Manténlo compacto: lo cerrado se borra de aquí (queda en git y en
  `docs/tickets/LOG.md`), no se acumula.

## Norte del proyecto

Un juego lo más parecido posible a Zelda Breath of the Wild. Roadmap de
contenido declarado por el usuario (2026-07-17): agrandar el mundo, más
modelos, cortar árboles, animales, más personajes, monturas, enemigos.

**Principio estructural:** IA y actores remotos se mueven **solo** escribiendo
`Intents` — nunca `Transform`, `BodyVelocity`, `LocomotionState`, facts ni
estado privado de motores. Ver `rationale/multi-actor-dispatch.md`.

## Estado actual (2026-07-17)

Fases Movement y Combat MVP validadas jugando. Implementado y commiteado:
locomoción por capacidades multi-actor, enemigos (percepción gradual,
melee + arquero), health/muerte, mounts (horse como `Actor`, lifecycle,
carga, inmunidad de dueño), espada con combos, arco de dos fases estándar
(pivote de mira a altura de ojos, socket del arco, fallbacks a la línea de
mira), modelo del player `Prototype.glb` con 18 clips y navegador de
animaciones de debug (F7, `[`/`]`).

Auditoría adversarial de arquitectura (2026-07-17, `docs/audits/`): 4
hallazgos, 4 corregidos el mismo día — input de orientación movido a
`PreUpdate` (era 1 frame tarde para `FixedUpdate`), patrón cross-schedule de
capacidad eliminado (`CapacityPending` ya no existe; los link-requests
aplican el mismo tick), percepción generalizada a `Perceivable` (marcador),
ventana de 1 tick del veto `ForbidSprint` fijada con test de regresión.

## Trabajo activo

**Separar capas de abstracción** (pedido del usuario, 2026-07-17): primer
paso — dividir `visuals.rs` (monolito de presentación) en submódulos y
extraer el layout del mundo de `world.rs` a datos, como fundación para
"agrandar el mundo". Deudas anotadas con dirección clara:

- Mundo-como-datos: el contenido de `world.rs` (cajas, escaleras, props) debe
  ser datos/escenas, no código Rust, antes de crecer el mapa.
- Animación generalizada: `animate_player` es específico del player; al
  llegar el segundo modelo riggeado (horse/enemigo), el mapeo estado→clip
  debe volverse por-arquetipo.
- Facciones: `Perceivable` es un bit; se reemplaza por facción cuando el
  gameplay necesite hostilidad entre no-jugadores.
- Ítems/inventario: no existe; llega con "cortar árboles" (la madera debe ir
  a alguna parte). El patrón de objeto destructible ya existe
  (`PracticeTarget` + `Health` + reacción de muerte del dueño).

## Invariantes a preservar

- `LocomotionState` exclusivo por actor; solo el motor activo escribe
  movimiento en un tick (tests `arbitration_matrix` y de aislamiento lo
  fijan).
- `Intents` es la frontera de control de player, IA y red.
- Facts/sensores separados de la ejecución de motores.
- Todo lo que la simulación de `FixedUpdate` lee del hardware se resuelve en
  `PreUpdate` (nunca `Update` — corre después de `FixedUpdate` en el frame).
- Presentación solo lee simulación; entidades que otro sistema puede
  despawnear el mismo frame usan comandos tolerantes (`try_insert`).
- El ordering de schedules y el árbitro de transiciones quedan intactos.
