# Plan: capacidades flexibles de Movement y lifecycle rider–mount

**Estado:** Tickets A–H implementados; checkpoint jugado final pendiente.

## Pre-implementation Checklist

- [x] Constitución y Architecture Map releídos.
- [x] Alcance A–E confirmado por el orquestador y entregado por checkpoints.
- [x] Cambios ajenos y `assets/Prototype.glb` preservados.
- [x] Sin crate nuevo.
- [x] Checks focalizados y suite completa (185 tests) ejecutados.

**Cierre automatizado 2026-07-16:** `cargo fmt --check`, `cargo check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` (185/185) y
`git diff --check` limpios. El checkpoint jugado permanece abierto.

## Objetivo

Mantener al horse como `Actor` de Movement y reutilizar pipeline, facts,
stairs, arbitraje y helpers físicos, sin tratarlo como un humano con tuning
distinto. Corregir al mismo tiempo ownership, lifecycle, scheduling, combate,
inmunidad, carga, presentación y documentación del slice montado.

Fuentes de verdad:

1. `docs/CONSTITUTION.md`.
2. `docs/audits/mounts-actor-refactor-cold-audit.md`.
3. `docs/audits/mounts-actor-refactor-validated-analysis.md`.
4. `docs/architecture/rationale/movement-capability-composition.md`.
5. `docs/architecture/rationale/mounted-actor-ownership.md`.

## Principios fijados

- Horse permanece como `Actor` de Movement.
- Reutilización ocurre por pipeline, facts, kernels y capacidades; no por
  branches de especie.
- Presencia de componente concede capacidad; sus datos configuran dinámica.
- Ground, Sprint, Sneak y Stairs son capacidades independientes.
- Movement es el único writer de cuerpos `Actor`, incluso attachments.
- Mounts posee la relación y reglas de mount, no `Transform`/`BodyVelocity`.
- Cada mount tiene como máximo un rider y viceversa.
- Todo borde entre plugins tiene ordering explícito.
- Los nuevos paths de link/charge no construyen colecciones, colliders ni
  filtros con exclusiones en `FixedUpdate`; capacidad y shapes se preparan en
  `Update` y los tests del path real fijan que la capacidad no cambie.
- El comportamiento Player validado se preserva durante la migración; el
  feeling Horse se valida después de que la arquitectura sea correcta.
- Sin crate nuevo.

## Estrategia de entrega

No implementar como un único diff. El trabajo se divide en tickets que dejan
el proyecto compilable y con invariantes protegidas en cada corte.

```text
A. Contratos/tests de schedule
  -> B. Capacidades terrestres granulares
  -> B2. Núcleo mínimo del actor
  -> C. Drive profile/kernel flexible
  -> D. Attachment/control Movement-owned
  -> E. Lifecycle Mounts uno-a-uno y safe dismount
  -> F. Combat/owner/inmunidad
  -> G. Charge/presentación/debug
  -> H. Docs, suite y checkpoint
```

## Ticket A — `mounted-schedule-contracts`

### Alcance

Crear primero handles de sets y tests de `App` que reproduzcan los bugs de
integración actuales sin cambiar todavía el feeling.

### Cambios

1. Declarar `MountsSet` con fases nombradas para request, lifecycle, post-move,
   charge y death cleanup.
2. Agregar a Movement fases públicas necesarias para aplicar cambios externos
   y sincronizar attachments; inicialmente pueden estar vacías.
3. Expresar el orden completo con Combat, Projectiles y Health.
4. Agregar tests con un `App` mínimo y dos o más fixed ticks; no depender solo
   de `run_system_once`.
5. Pinnear la latencia aceptada de cada message channel.

### Invariantes/tests

- Redirect ocurre antes de Sense/Gather.
- Sync de rider ocurre antes de cualquier lectura de Combat.
- Charge emite antes de `HealthSet::Apply`.
- Contexto montado se aplica determinísticamente antes de Combat o con una
  latencia fija documentada.
- Plugin registration order no cambia resultados.

### File touches probables

- `src/movement/mod.rs`
- `src/mounts/mod.rs`
- `src/combat/mod.rs`
- `src/projectiles/mod.rs`
- `src/health/mod.rs`
- Tests de integración nuevos bajo el módulo dueño.

## Ticket B — `movement-ground-capability-split`

### Alcance

Separar permiso y estado runtime de Ground/Sprint/Sneak/Stairs sin cambiar aún
la fórmula física validada del Player.

### Cambios

1. Reducir `GroundMovement` al drive terrestre normal.
2. Crear componentes de datos `SprintMovement`, `SneakMovement` y
   `StairsMovement`.
3. Dividir `GroundMovementBundle` en bundles por capacidad.
4. Mover `SprintLock` al bundle Sprint.
5. Mover crouch, `SneakLock`, clearance y colliders al bundle Sneak.
6. Mover `StairsLocal`/`StairsFacts` al bundle Stairs.
7. Cambiar cada proposal/tick/service para requerir su capacidad exacta.
8. Componer Player con las cuatro capacidades.
9. Componer Horse con Ground + Sprint + Stairs y sin Sneak.
10. Actualizar Enemies/Probe con capacidades explícitas según su contrato.
11. Separar `GaitIntent` en solicitudes ortogonales de intensidad/sprint y
    postura/sneak, o documentar explícitamente una fase de compatibilidad; no
    conservar ambos conceptos unidos como contrato final.
12. Mantener `Intents` como snapshot semántico amplio; la capacidad es el gate
    final aunque exista una solicitud incompatible.

### Invariantes/tests

- Horse carece de `SneakMovement`, crouch data y stand collider alternativo.
- Intents Sneak sobre un actor sin capacidad no producen propuesta ni cambio de
  collider.
- Actor sin Sprint/Stairs tampoco propone esos estados.
- Player conserva valores y resultados anteriores.
- Tests de composición verifican ausencia y presencia de cada capacidad.

### File touches probables

- `src/movement/abilities.rs`
- `src/movement/bundles.rs`
- `src/movement/motors/{walk,sprint,sneak,stairs}.rs`
- `src/movement/services/stairs.rs`
- `src/movement/motors/mod.rs`
- `src/player.rs`
- `src/enemies/mod.rs`
- `src/movement/probe.rs`
- `src/mounts/data.rs` / composición del horse.

## Ticket B2 — `movement-minimal-actor-core`

### Alcance

Reducir `KinematicActorBundle` al contrato obligatorio y mover datos opcionales
a los pools/capacidades que realmente los consumen. Puede implementarse como
subtickets mecánicos si las queries afectadas vuelven demasiado grande el diff.

### Cambios

1. Sacar Stamina del núcleo obligatorio y crear su bundle/pool explícito.
2. Mover `LedgeFacts`, `StairsFacts` y `LadderFacts` a sus contratos de
   sensing/capacidad correspondientes.
3. Auditar cada campo del bundle base: si un actor válido puede existir sin él,
   no pertenece al núcleo.
4. Adaptar `MotorTickItem`/queries sin convertir todos los opcionales en una
   interfaz gigante; dividir queries o dispatch cuando corresponda.
5. Mover el pago `JumpStaminaCost` fuera de `jump::propose`: cobrar solo cuando
   Jump ganó el arbitraje y la transición fue aceptada.
6. No cobrar stamina si el proposal perdió o fue rechazado por overflow.

### Invariantes/tests

- Actor terrestre sin stamina es representable si sus capacidades no gastan.
- Agregar Stamina no concede Sprint/Jump por sí mismo.
- Actor sin Ladder/Ledge/Stairs no carga sus facts/local state.
- Jump derrotado por WallJump/Mantle/EdgeLeap no consume stamina.
- Proposal rechazado por overflow no produce side effects.
- Los actores existentes conservan pools/facts que realmente necesitan.

### File touches probables

- `src/movement/bundles.rs`
- `src/movement/facts.rs`
- `src/movement/motors/mod.rs`
- `src/movement/motors/jump.rs`
- Servicios/motores que hoy exigen facts o stamina globalmente.
- Composición en Player, Enemies, Probe y Horse.

## Ticket C — `movement-ground-drive-profile`

### Alcance

Reemplazar `acceleration + friction` como aproximación universal por un kernel
de drive capaz de representar Player, Horse y actores pesados sin branches de
dominio.

### Cambios

1. Introducir `GroundDriveProfile` solo con campos consumidos por el nuevo
   helper: velocidades forward/reverse, aceleración forward/reverse, coast,
   brake, alineación de velocity, turn rate dependiente de velocidad y pérdida
   de velocidad al girar.
2. Separar explícitamente input cero, input alineado, braking e inversión.
3. Implementar `ground_drive_step` como helper común sin allocations.
4. Hacer que Ground, Sprint y Sneak seleccionen su profile y llamen el mismo
   kernel.
5. Crear un preset de compatibilidad Player que preserve el checkpoint actual.
6. Crear presets Horse con arranque progresivo, frenado largo, baja respuesta
   lateral y steering limitado a alta velocidad.
7. Ajustar Stairs para consumir su capacidad/perfil y conservar facts comunes.
8. No crear `if Horse` ni importar markers de dominio en Movement.
9. Si el checkpoint posterior demuestra que el kernel no representa un cuerpo
   no holonómico, abrir un ticket aditivo `steered-ground-movement`; no inflar
   este kernel con excepciones.

### Invariantes/tests

- Curvas deterministas de aceleración, coast, brake, reversa y giro.
- Igual input produce respuestas distintas por profile.
- Valores Player quedan dentro de tolerancia del comportamiento previo.
- No hay dependencia Movement → Horse/Enemy/Player.
- Ningún helper asigna en heap.

### Checkpoint

- Player: caminar, frenar, invertir, sprintar, rampa y stairs sin regresión.
- Horse: salida, galope, giro a baja/alta velocidad, frenado e inversión.

## Ticket D — `movement-actor-attachment`

### Alcance

Crear contratos genéricos Movement-owned para suspender, portar y controlar un
Actor, reutilizables por mounts, vehículos y plataformas.

### Datos separados de sistemas

- Datos/mensajes en archivos propios (`attachment.rs`/`control.rs` o
  `data.rs` según responsabilidad).
- `KinematicAttachment { carrier, local_pose }`.
- Marker Movement-owned que habilita la locomoción física desde SenseWorld.
- Request de attach/detach con pose/velocity semánticas.
- Estado persistente de redirect y release explícito con máscara de controles.

### Cambios

1. Mantener `Actor` en el rider para Combat/Camera/Input.
2. Excluir actors adjuntos de Sense/Gather/Arbitrate/Tick mediante un contrato
   Movement-owned, no mediante checks de Mounts.
3. Permitir que `brain::read_intents` siga alimentando al controller adjunto.
4. Redirigir planar, boost y jump al controlled actor y neutralizar controller.
5. En release, neutralizar controlled, limpiar redirect y reactivar rider.
6. Sincronizar attachments dentro de Movement después del carrier motor y
   antes de Combat.
7. Validar self-attachment, entidades faltantes, destino duplicado y ciclo no
   soportado sin panic.
8. Definir comportamiento ante carrier desaparecido: detach/recovery, nunca
   actor permanentemente sin collider.

### Invariantes/tests

- Solo Movement escribe `Transform`/`BodyVelocity` del rider.
- Rider adjunto no ejecuta proposals/motores ni avanza estado privado.
- Horse sí ejecuta el pipeline normal.
- Release deja ambos `Intents` en estado definido.
- Attach/detach con entidad inválida no deja estado parcial.
- Tests multi-actor cubren redirects simultáneos sin last-writer-wins.

## Ticket E — `mounts-lifecycle-rebuild`

### Alcance

Reescribir Mounts sobre los contratos anteriores y separar datos/lógica según
Constitución §19.

### Organización objetivo

- `src/mounts/data.rs`: `Horse`, `MountedOn`, `RiddenBy`, charge state.
- `src/mounts/lifecycle.rs`: mount/dismount/death/orphan reconciliation.
- `src/mounts/control.rs`: emisión de contratos Movement-owned.
- `src/mounts/charge.rs`: detección y mensajes de charge.
- `src/mounts/debug.rs`: request F8, sin mutación de simulación en `Update`.
- `src/mounts/mod.rs`: plugin y sets solamente.
- Presentación del horse fuera de los archivos de simulación.

### Cambios

1. Agregar `RiddenBy` y hacer la relación uno-a-uno.
2. Centralizar E, F8, muerte y orphan cleanup en una transición común.
3. Usar cursor Mounts propio, separado de Movement/Combat.
4. E normal selecciona mount libre dentro de alcance.
5. Attach/detach se pide a Movement; Mounts no escribe cuerpos/colliders.
6. Al desmontar se libera relación, redirect, attachment, contexto e
   inmunidad/ownership según sus contratos.
7. Detectar desaparición inesperada de cualquiera de los extremos.
8. Mantener un horse muerto como pending despawn si hace falta un tick para
   completar detach seguro; nunca despawn antes de liberar el rider.

### Safe dismount

1. Probar un arreglo fijo de candidatos alrededor del horse.
2. Validar cápsula, headroom, suelo y pendiente con `SpatialQuery`.
3. Excluir rider/horse.
4. Dismount voluntario sin candidato se rechaza.
5. Dismount forzado usa búsqueda radial fija y fallback sin overlap desde el
   que el rider pueda caer.
6. Heredar velocity planar del carrier.

### Invariantes/tests

- Simetría completa mount/dismount.
- Dos riders no ocupan un mount.
- Rider no ocupa dos mounts.
- Pared derecha/izquierda, borde y headroom bloqueado.
- E, F8, muerte y despawn inesperado dejan el mismo estado final válido.

## Ticket F — `mounted-combat-and-owner-policy`

### Combat

1. Separar datos y sistemas de `combat/context.rs`.
2. `CombatContext` no guarda ni reemplaza `WeaponProfile` base.
3. Resolver un effective profile read-only para espada/arco.
4. Snapshottear `AttackStep`/tuning al iniciar Windup y bow tuning al iniciar
   draw; el contexto no altera una acción activa.
5. Aplicar cambios de contexto antes del pipeline Combat con ordering fijado.
6. Conservar extensibilidad por tipo de mount sin importar `mounts::Horse`.

### Owner/inmunidad

1. Modelar owner persistente separado de rider, o cambiar explícitamente el
   requisito si producto desea inmunidad solo al rider actual.
2. Decidir y nombrar la semántica: HP immunity o full hostile-interaction
   immunity.
3. Si es HP-only, Health emite resultado aplicado/rechazado y feedback/impulso
   reaccionan al resultado.
4. Si es full interaction, exponer un contrato con ese nombre y documentar las
   lecturas de Combat/Projectiles/Health.
5. Health conserva la validación autoritativa; hazards con `None` siguen
   funcionando.

### Invariantes/tests

- Equipar un arma, montar y desmontar no cambia el arma base.
- Context switch en Windup/Active/Recovery no altera el ataque iniciado.
- Sword/arrow del owner cumplen la semántica elegida; enemigo y hazard dañan.
- Cambio de rider no redefine owner accidentalmente.

## Ticket G — `horse-charge-presentation-debug`

### Charge

1. Reemplazar scan global por overlap/sweep espacial del volumen del horse.
2. Evitar golpes a través de geometría y tunneling a velocidad alta.
3. Eliminar el límite implícito de 16 objetivos por carga o reemplazarlo por un
   límite de dominio explícito con política observable y test de overflow.
4. Mantener deduplicación por `(horse, charge_generation, enemy)` sin
   allocations en FixedUpdate.
5. Agregar hysteresis si el threshold 11 m/s oscila durante contacto.
6. Emitir daño antes de Health e impulso con latencia determinista.

### Presentación/debug

1. Mover assets/mesh/material/cues a Presentation/Visuals.
2. El horse de simulación no depende de componentes visuales.
3. F8 desde Update solo emite request; FixedUpdate resuelve spawn/despawn.
4. Justificar/referenciar `assets/Prototype.glb` o retirarlo del slice mediante
   una decisión humana separada; no borrarlo automáticamente.

### Tests/checkpoint

- Un Enemy por carga, rearm, objetivo 17+, dos cargas/horses según soporte.
- Sweep a alta velocidad, pared entre horse/enemy y diferencia vertical.
- Presentación sobre entidad despawneable usa comandos tolerantes.

## Ticket H — `mounted-refactor-closeout`

### Documentación

1. Actualizar `movement.md`, `mounts.md`, `combat.md`, `health.md`, `input.md`,
   `ARCHITECTURE-MAP.md`, `COUPLING-MAP.md` y `WORKING-CONTEXT.md` al estado
   realmente implementado.
2. Corregir referencias falsas a `DamageAppliedMessage`, migración Combat y
   `Parry` no implementado.
3. Marcar los tickets reemplazados como superseded, sin reescribir historia.
4. Registrar latencias de messages y ownership final.

### Verificación automatizada

- `cargo fmt --check`.
- `cargo check`.
- `cargo clippy --all-targets -- -D warnings`.
- `cargo test`.
- `git diff --check`.
- Tests de App/schedule y composición incluidos en tickets anteriores.
- Test/instrumentación de no allocations para los nuevos hot paths de drive,
  attachment, redirect y charge.

### Checkpoint jugado final

- F8 spawn/despawn.
- E mount/dismount en espacio libre y bloqueado.
- Horse walk/sprint/jump/stairs con steering e inercia propios.
- Horse no puede Sneak ni recibe capacidades humanoides.
- Charge contra grupos, paredes y a alta velocidad.
- Sword/bow montados; cambio de contexto durante acción.
- Owner immunity según semántica elegida.
- Daño enemigo, muerte del horse y cleanup sin referencias colgantes.
- Player a pie conserva locomoción y combate previos.

## Riesgos de implementación

- Cambiar filtros `With<Actor>` a una fase de locomoción habilitada toca muchos
  motores/servicios; debe hacerse mecánicamente y protegerse con matriz/tests.
- Separar Stamina/facts del bundle núcleo puede ampliar demasiado el primer
  ticket. Hacerlo después del capability split si rompe demasiadas queries.
- Un kernel de drive demasiado configurable puede convertirse en un motor con
  flags. Si aparecen branches cualitativos, cortar y crear otro motor.
- Commands diferidos pueden reintroducir latencia accidental; cada transición
  debe tener test de App y ordering explícito.
- La relación owner no debe confundirse con `MountedOn` por conveniencia.

## Definition of Done global

- No quedan findings confirmados del análisis validado sin resolución o
  decisión documentada.
- Constitución §§1, 5, 7, 11, 18, 19 y 20 se cumplen estructuralmente.
- Horse reutiliza Movement sin acceso a Sneak/crouch y sin branch de especie.
- Player/Horse demuestran perfiles dinámicos distintos sobre kernels comunes.
- Movement es único writer de rider y horse bodies.
- Lifecycle y schedule están cubiertos por tests reales de App.
- Documentación describe el código presente, no el objetivo futuro.
- Checkpoint jugado aceptado por el usuario.

### Fidelity Check — iteración B+B2+C

| Step | Location | Notes |
| :--- | :--- | :--- |
| Ticket B | `src/movement/abilities.rs:28`, `src/movement/bundles.rs:95` | Ground, Sprint, Sneak y Stairs son capacidades y bundles independientes; Player/Horse/Enemy/Probe se recomponen explícitamente. |
| Ticket B2 | `src/movement/bundles.rs:40`, `src/movement/motors/jump.rs:107` | El core deja fuera Stamina y facts opcionales; el costo de Jump se paga solo al aceptar el estado. |
| Ticket C | `src/movement/abilities.rs:7`, `src/movement/motor_common.rs:284` | `GroundDriveProfile` y el kernel común modelan aceleración, coast, brake, reversa, alineación y giro con presets Player/Horse. |

### Fidelity Check — iteración A+D+E

| Step | Location | Notes |
| :--- | :--- | :--- |
| Ticket A | `src/movement/mod.rs`, `src/mounts/mod.rs` | Los sets fijan lifecycle → apply external → redirect → locomoción → sync attachment; los tests de `App` ejecutan el schedule real durante varios ticks. |
| Ticket D | `src/movement/attachment.rs`, `src/movement/control.rs`, `src/movement/attachment_systems.rs` | Movement posee attach/detach, suspensión por `LocomotionEnabled`, redirect persistente, neutralización, sync posterior al motor y recuperación huérfana. |
| Ticket E | `src/mounts/data.rs`, `src/mounts/lifecycle.rs`, `src/mounts/debug.rs` | Mounts posee relación uno-a-uno, cursor propio, selección con E, safe dismount, F8 en FixedUpdate y cleanup por muerte/despawn inesperado. |

### Fidelity Check — iteración F+G+H

| Step | Location | Notes |
| :--- | :--- | :--- |
| Ticket F | `src/combat/context.rs`, `src/combat/motors/attack.rs`, `src/combat/motors/aim.rs` | Combat deriva perfiles mounted read-only y snapshottea espada/arco sin reemplazar el `WeaponProfile` base. |
| Ticket F owner | `src/health/data.rs`, `src/mounts/lifecycle.rs`, `src/projectiles/mod.rs` | Owner persistente y `HostileInteractionImmunity` suprimen HP, feedback, threat e impulso por fuente; Health revalida. |
| Ticket G charge | `src/mounts/charge.rs`, `src/mounts/mod.rs` | Sweep espacial con oclusión, histéresis y ledger `(horse,generation,enemy)` reservado en Update; Charge precede Health. |
| Ticket G presentation/debug | `src/visuals.rs`, `src/mounts/debug.rs` | Horse visual separado y tolerante a orphan; F8 solo captura en Update y resuelve lifecycle en FixedUpdate. |
| Ticket H | `docs/architecture/{movement,mounts,combat,health,input}.md`, `docs/{ARCHITECTURE-MAP,COUPLING-MAP,WORKING-CONTEXT}.md` | Contratos, ownership, latencias, perfiles montados y features aún no implementadas reconciliados con código presente. |

## Pre-implementation checklist

- [x] Constitución leída.
- [x] Auditoría fría preservada.
- [x] Hallazgos validados por separado.
- [x] Decisión de composición documentada.
- [x] Decisión de ownership montado documentada.
- [x] Semántica owner/inmunidad fijada por el plan: interacción hostil completa frente al owner persistente.
- [x] Plan y orden de tickets confirmados por el usuario.
- [x] A–E coordinados por el orquestador sin solapar ownership de archivos.

## Correcciones 2–3 — alcance y File Touches exactos

La corrección se limita a los findings de la auditoría: transacción/cleanup de
mount, hot paths sin crecimiento, side effects de Jump, query exacta de
Movement, separación §19, steering y pruebas de integración. No agrega crates,
features de juego ni assets.

Inventario exhaustivo de `git status --short` para todo el worktree del slice:

- Documentación modificada: `docs/ARCHITECTURE-MAP.md`,
  `docs/COUPLING-MAP.md`, `docs/WORKING-CONTEXT.md`,
  `docs/architecture/combat.md`, `docs/architecture/health.md`,
  `docs/architecture/input.md`, `docs/architecture/mounts.md`,
  `docs/architecture/movement.md`,
  `docs/architecture/rationale/ecs-design-review.md`,
  `docs/architecture/rationale/enemy-damage-aggro.md`,
  `docs/architecture/rationale/health-ownership-boundary.md`,
  `docs/architecture/rationale/mounts-intent-redirect.md`,
  `docs/architecture/rationale/movement-capability-composition.md`,
  `docs/architecture/rationale/multi-actor-dispatch.md`,
  `docs/architecture/rationale/proposal-arbitration-core.md` y
  `docs/tickets/mounts-core.md`.
- Documentación nueva: `docs/architecture/rationale/mounted-actor-ownership.md`,
  `docs/audits/mounts-actor-refactor-cold-audit.md`,
  `docs/audits/mounts-actor-refactor-validated-analysis.md` y
  `docs/implement-feature/movement-capabilities-and-mount-lifecycle-plan.md`.
- Combat modificado: `src/combat/mod.rs`, `src/combat/motors/aim.rs`,
  `src/combat/motors/attack.rs`, `src/combat/state.rs` y
  `src/combat/weapon.rs`.
- Combat nuevo: `src/combat/context.rs` y `src/combat/context_data.rs`.
- Enemies/Health/Input modificados: `src/enemies/brain.rs`,
  `src/enemies/mod.rs`, `src/health/data.rs`, `src/health/mod.rs`,
  `src/input/action.rs` y `src/input/mod.rs`.
- Integración modificada: `src/main.rs`, `src/player.rs`,
  `src/projectiles/mod.rs` y `src/visuals.rs`.
- Movement modificado: `src/movement/abilities.rs`,
  `src/movement/brain.rs`, `src/movement/bundles.rs`,
  `src/movement/constraints.rs`, `src/movement/intents.rs`,
  `src/movement/mod.rs`, `src/movement/motor_common.rs`,
  `src/movement/probe.rs`, `src/movement/services/ground.rs`,
  `src/movement/services/ladder.rs`, `src/movement/services/ledge.rs` y
  `src/movement/services/stairs.rs`.
- Motores Movement modificados: `src/movement/motors/auto_vault.rs`,
  `src/movement/motors/climb.rs`, `src/movement/motors/edge_leap.rs`,
  `src/movement/motors/fall.rs`, `src/movement/motors/glide.rs`,
  `src/movement/motors/jump.rs`, `src/movement/motors/ladder.rs`,
  `src/movement/motors/mantle.rs`, `src/movement/motors/mod.rs`,
  `src/movement/motors/sneak.rs`, `src/movement/motors/sprint.rs`,
  `src/movement/motors/stairs.rs`, `src/movement/motors/walk.rs` y
  `src/movement/motors/wall_jump.rs`.
- Movement nuevo: `src/movement/attachment.rs`,
  `src/movement/attachment_systems.rs`, `src/movement/control.rs` y
  `src/movement/link.rs`.
- Mounts nuevo: `src/mounts/charge.rs`, `src/mounts/charge_data.rs`,
  `src/mounts/control.rs`, `src/mounts/data.rs`, `src/mounts/debug.rs`,
  `src/mounts/lifecycle.rs` y `src/mounts/mod.rs`.
- Fuera del diff de implementación: `assets/Prototype.glb` ya estaba
  preexistente y untracked; se preserva sin modificar, incorporar ni borrar.

La instrumentación segura comprueba invariancia de capacidad antes/después de
los sistemas reales. Un contador global de allocations requeriría implementar
`GlobalAlloc` con `unsafe` (prohibido por §12) o incorporar un crate nuevo
(requiere aprobación por §17), por lo que no se afirma esa cobertura.

### Fidelity Check — correcciones 2–3

| Step | Location | Notes |
| :--- | :--- | :--- |
| Atomicidad | `src/movement/link.rs:7`, `src/movement/attachment_systems.rs:37` | Un request instala o retira attachment, redirect, collider y gate como una transacción con ack/rechazo. |
| Cleanup/fallback | `src/mounts/lifecycle.rs`, `src/movement/attachment_systems.rs` | Mounts libera la relación; una pose no validada queda con collider/locomoción suspendidos hasta el recovery Movement-owned. |
| FixedUpdate | `src/movement/link.rs:48`, `src/mounts/charge_data.rs:9`, `src/mounts/charge.rs:48` | Workspaces, ledger y collider persistentes se preparan en Update; `CapacityPending` conserva el sweep. |
| Jump/B2 | `src/movement/motors/jump.rs:50`, `src/movement/motors/mod.rs:33` | Overflow no cambia estado privado y cada motor consulta solo su capacidad/facts/pool exactos. |
| §19 | `src/combat/context_data.rs:1`, `src/combat/context.rs:1`, `src/mounts/charge_data.rs:1`, `src/mounts/charge.rs:1` | CombatContext y charge ledger son datos puros en archivos distintos de sus sistemas. |
| Retry lossless | `src/movement/attachment_systems.rs`, `src/mounts/lifecycle.rs` | `CapacityPending` reencola Attach/Detach/Neutralize y F8/Death preservan `PendingHorseDespawn` hasta confirmar. |
| Integración | `src/mounts/mod.rs` | Apps con plugins y física reales fijan pose+context same-tick en dos órdenes y prueban el registro productivo de motores. |
| Verificación | `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings` | 185 tests pasan y Clippy estricto está limpio; checkpoint jugado continúa pendiente. |
