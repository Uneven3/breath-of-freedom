# Coupling Map

`ARCHITECTURE-MAP.md` responde *quién habla con quién*. Este documento
responde la pregunta que hace falta para repartir tickets/worktrees:
**qué tan caro es tocar dos sistemas en paralelo sin coordinar antes de
escribir código.** Mapea las **210 combinaciones** posibles entre los 21
sistemas documentados (una vez, sin repetir simétricamente).

## Escala

| Símbolo | Nivel | Qué significa | Categoría de `ARCHITECTURE-MAP.md` |
|---|---|---|---|
| **T** | Tight | Cambiar la forma de uno rompe al otro en compilación o en runtime silenciosamente. Hay que acordar el contrato *antes* de escribir código, o construirlos en el mismo ticket/secuencia. | `SHARED-CONTRACT`, `BLOCKING-PREREQUISITE` |
| **M** | Middle | Uno escribe a partir de leer al otro, cruzando un límite de *ordering* de schedule. Paralelizable si el contrato (forma del dato leído) se fija primero. | `WRITE-OWN` |
| **L** | Loose | Solo lectura read-only o mensajes desacoplados. Cualquiera de los dos lados cambia su implementación interna libremente. | `READ`, `MESSAGE` |
| **?** | Abierto | Un doc lo menciona en "Decisiones abiertas" nombrando al otro sistema, pero no está diseñado. Ningún ticket debería asumir su forma todavía. | — |
| **=** | Mismo sistema | No son dos sistemas distintos — viven en el mismo plugin. | — |
| *(vacío)* | Sin relación | Nada documentado entre ambos. | — |

## Matriz (triángulo superior — cada par aparece una sola vez)

`Dbg` (Debug) es Loose con los 20 restantes por construcción — lee todo,
como UI/Camera — así que su columna es toda `L` y no suma una fila propia.

| | Cmb | Wld | Ene | Mnt | MPl | UI | SFX | VFX | Cam | Hlt | Prj | Inv | Crf | Swm | Snw | Sts | NPC | Per | Inp | Dbg |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Mov** | T | L | T | T | T | L | L | L | L | | | | | = | = | L | | M | M | L |
| **Cmb** | | | T | T | T | L | L | L | L | L | L | L | | ? | ? | | | | M | L |
| **Wld** | | | L | | T | | L | L | | | | ? | ? | L | L | L | L | M | | L |
| **Ene** | | | | L | ? | | | | ? | L | | ? | | | | | ? | | | L |
| **Mnt** | | | | | T | L | | | | L | | | | | | | | | | L |
| **MPl** | | | | | | | | | | | T | | | | | | T | ? | T | L |
| **UI** | | | | | | | | | | L | | L | L | | | | L | L | L | L |
| **SFX** | | | | | | | | | | | | | | | | L | | | | L |
| **VFX** | | | | | | | | | | | | | | | | L | | | | L |
| **Cam** | | | | | | | | | | | | | | | | | | | L | L |
| **Hlt** | | | | | | | | | | | T | | | L | | L | ? | M | | L |
| **Prj** | | | | | | | | | | | | | | | | | | | | L |
| **Inv** | | | | | | | | | | | | | T | | ? | L | L | M | | L |
| **Crf** | | | | | | | | | | | | | | | | ? | | | | L |
| **Swm** | | | | | | | | | | | | | | | = | L | | | | L |
| **Snw** | | | | | | | | | | | | | | | | L* | | | | L |
| **Sts** | | | | | | | | | | | | | | | | | | | | L |
| **NPC** | | | | | | | | | | | | | | | | | | M | M | L |
| **Per** | | | | | | | | | | | | | | | | | | | M | L |
| **Inp** | | | | | | | | | | | | | | | | | | | | L |

(`Cmb`-`Snw`/`Swm` marcados `?`: si se puede atacar mientras se nada/hace
snowboard. `Sts` en fila `Snw` marcada `L*`: relación confirmada pero
mecanismo — READ o MESSAGE — sin decidir.)

## Pares Tight — coordinar antes de tickets separados

- **Movement↔Combat↔Mounts** (los 3 entre sí): comparten la *forma* del
  núcleo de arbitración `proposal::ProposalBuffer<S, N>`
  (`rationale/proposal-arbitration-core.md`). Un cambio a esa forma rompe
  a los tres a la vez aunque Combat y Mounts nunca se mencionen entre sí.
- **Combat→Enemies (sigilo/aggro):** Combate lee `enemies::Awareness` del
  objetivo (bonus contra no-alertados) y emite `enemies::DirectThreatMessage`
  al conectar — el mensaje es propiedad del receptor (Enemies), así que el
  contrato quedó fijado antes de que Combat exista.
- **Movement↔Enemies, Combat↔Enemies:** mismo `Intents`/`CombatIntents`,
  más el prerequisito bloqueante del contrato multi-actor
  (`rationale/multi-actor-dispatch.md`) — Enemies no se puede empezar en
  serio hasta que ese refactor exista.
- **Movement↔Multiplayer, Combat↔Multiplayer, Mounts↔Multiplayer,
  World↔Multiplayer, Projectiles↔Multiplayer, NPCs↔Multiplayer:** mismo
  contrato multi-actor/host-autoritativo — todos comparten el patrón "solo
  el host simula, el cliente replica" (`rationale/multiplayer-gating.md`).
- **Multiplayer↔Input:** `LocalInputFrame` empaqueta `input::ActionFrame` ya
  resuelto; Multiplayer emite `ApplyRemoteActionsMessage` e Input escribe su
  propio `ActiveActions` del `InputSource` de red. Un cambio de forma en
  `ActionFrame`/`LocalInputFrame`/`ApplyRemoteActionsMessage` rompe red e
  input a la vez (`input.md`, `multiplayer.md`).
- **Crafting↔Inventory:** `BLOCKING-PREREQUISITE` — Crafting no existe sin
  el modelo de ítems de Inventory.
- **Health↔Projectiles:** `BLOCKING-PREREQUISITE` de secuencia (Projectiles
  necesita `Health` para poder emitir daño), pero la relación *en curso*
  una vez construidos ambos es apenas un `MESSAGE` (loose) — este par se
  afloja después del primer build, a diferencia de los anteriores.

## Pares Middle — fijar el contrato, después paralelizable

- **Mounts↔Movement:** Mounts emite `ActorLinkRequestMessage` y confirma su
  relación solo desde `ActorLinkResultMessage`; Movement aplica attachment,
  redirect, collider y gate atómicamente y es el único escritor de cuerpos e
  `Intents` durante transferencia y sync.
- **Input↔{Movement, Combat, NPCs}:** esos sistemas leen `ActiveActions` y
  escriben su propio `InputConsumeCursor` para gatillos discretos. El
  contrato a fijar es la forma de `IntentAction`, `InputSource` y el cursor;
  ninguno toca `Keybindings`.
- **Persistence↔{Movement, Health, Inventory, NPCs, World, Input}:** cada
  uno registra su propio `save_x`/`load_x` en el `SystemSet` de Persistence
  (`rationale/persistence-orchestration.md`) — el contrato es el orden de
  esos sets, no un tipo compartido. `Input` es un caso algo distinto: lo
  que persiste es preferencia del jugador (`Keybindings`), no estado de
  partida, pero pasa por el mismo mecanismo (`input.md`).

## Pares abiertos (`?`) — resolver antes de asignar el ticket que los toque

Estos son exactamente los que conviene decidir *antes* de repartir trabajo,
para no descubrir a mitad de un ticket que dos sistemas se necesitan:

- Combat↔Swim/Dive, Combat↔Snowboard: ¿se puede atacar nadando o en tabla?
- Enemies↔Mounts: ¿los enemigos montan criaturas?
- Camera↔Enemies: criterio de selección de lock-on.
- Enemies↔Multiplayer: un enemigo en sesión multiplayer, ¿es un `Actor`
  replicado igual que un jugador remoto?
- Crafting↔StatusEffects: elixires que dan resistencia a frío/calor.
- Snowboard↔Inventory: ¿la tabla es un ítem craftable/equipable?
- Inventory↔Enemies, Inventory↔World: origen del loot (enemigos, cosecha,
  cofres).
- NPCs↔Enemies: ¿un NPC puede volverse hostil?
- NPCs↔Health: ¿tienen `Health` los NPCs?
- Multiplayer↔Persistence: ¿el snapshot de red y el archivo de guardado
  comparten formato de serialización?

## Pares Loose

Ya están detallados uno por uno en `ARCHITECTURE-MAP.md` § Cross-System
Dependencies — bajo riesgo, no requieren acordar nada antes de paralelizar,
solo que el lector se adapte si el emisor cambia de forma.

## Nota de alcance

Esta matriz describe la arquitectura objetivo conocida. Un sistema con
columna vacía puede ganar una relación real cuando se cierre una decisión
abierta (ej. StatusEffects puede terminar tocando Crafting). Volver a pasar
el grep de `docs/architecture/rationale/` sobre esta tabla cuando se cierre
una decisión abierta.
