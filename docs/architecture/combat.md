# Combat

**Carpeta objetivo:** `src/combat/`

**Estado:** fases 1-2 del plan implementadas (tickets `combat-scaffolding` y
`combat-melee-combo`, 2026-07-15) — pipeline completo + espada graybox de 3
pasos contra el bokobo, pendiente el checkpoint de feeling jugado. Daño real
espera `health-core` (hoy: cue de log).

## Feeling objetivo (GDD §7/§8)

Lento y con peso, timing deliberado — el feeling de BotW, no un
character-action rápido. Traducido a mecánica:

- **El peso está en los frames, no en la animación:** cada golpe tiene
  `Windup` (compromiso, no cancelable por otro ataque), `Active` (hitbox
  vivo) y `Recovery` (vulnerable). Leer al enemigo importa más que la
  velocidad de input porque el input te *compromete*.
- **Combos por arma:** cada clase de arma define su propia cadena de golpes
  (pasos con timing, daño y desplazamiento propios). El combo avanza solo si
  encadenás dentro de la ventana; si no, la cadena se reinicia. Pocos tipos
  de arma bien diferenciados por peso/velocidad/alcance — la diferencia se
  siente en la cadena, no en un catálogo.
- **El sigilo es un bonus, no un pilar aparte:** contrato ya fijado sobre
  `enemies::Awareness` (ver Relaciones).
- **Sin flurry rush / perfect dodge** (decidido 2026-07-15): no es necesario
  para el feeling buscado, y su dilatación de tiempo compromete la
  arquitectura del `FixedUpdate` — la prioridad es una buena arquitectura.
  La defensa es escudo (guard/parry), no esquiva recompensada.

## Alcance del MVP de combate (decidido 2026-07-15)

Una **espada normal** (una mano, cadena de 3 pasos), un **escudo**
(guard/parry), un **arco con flechas normales** (con Projectiles), el
**lock-on de cámara** (con Camera), **HP** (con Health), y **VFX de swing**
como placeholder mientras no haya animaciones de combate. Las clases
`TwoHanded`/`Spear` quedan soportadas por el modelo de datos (son un preset
const más) pero fuera del MVP.

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `CombatIntents` | `combat/intent.rs` | Propio, nombre distinguible del `Intents` de Movement: `attack: AttackIntent { pressed, held }` (el hold distingue golpe de cargado). `wants_guard`/`wants_parry`/`wants_aim` se agregan cuando un motor los lea (fases 5-6), no antes. Solo el Brain del actor lo escribe (`CombatInputCursor` propio — Movement ya posee un `InputConsumeCursor` en el mismo actor y dos consumidores no comparten cursor). |
| `CombatState` | `combat/state.rs` | Enum SSoT propio de Combate — ver Estados. Solo `arbitrate` de Combate lo escribe. |
| `CombatProposalBuffer` | `combat/proposal.rs` | Type alias sobre el núcleo genérico compartido de capacidad fija `proposal::ProposalBuffer<CombatState, N>` (`src/proposal.rs`) — ver `rationale/proposal-arbitration-core.md`. |
| `WeaponClass` | `combat/weapon.rs` | `OneHanded`, `TwoHanded`, `Spear` — pocas clases, bien diferenciadas (GDD §8). **Diferido a `combat-weapon-classes`**: ningún sistema lo lee todavía, y un campo que nadie lee no se agrega. |
| `AttackStep` | `combat/weapon.rs` | Un golpe de la cadena: `windup_secs`, `active_secs`, `recovery_secs`, `chain_window_secs`, `damage_mult`, `reach`, `arc_deg`, `motion` (empuje hacia adelante del paso, si lo hay). Datos puros (Constitución §6). |
| `WeaponProfile` | `combat/weapon.rs` | Capacidad + tuning del arma empuñada: `class`, `base_damage`, `combo: [AttackStep]` (array fijo por clase), `charged: AttackStep` (+ costo de stamina), `durability_per_hit`. Presets const (`WeaponProfile::GRAYBOX_SWORD`, patrón `GroundMovement::PLAYER`). En graybox vive directo en el actor; cuando exista Equipment (Inventory), equipar/desequipar = insertar/quitar este componente — el componente ES el booleano "está armado" (ver `rationale/per-entity-state-idioms.md`). |
| `ComboLocal` | `combat/motors/attack.rs` | Bookkeeping por-actor del combo: `step`, `phase_elapsed`, `buffered_attack`, `chain_deadline`. Componente, nunca `Local` de sistema (contrato multi-actor). |
| `ActiveSwing` | `combat/motors/attack.rs` | Dedup del golpe en curso: set de entidades ya golpeadas por este swing (capacidad fija) — un swing pega una sola vez por objetivo. |
| `LocomotionConstraintMessage` | **`movement/constraints.rs`** (corregido al implementar: el receptor es dueño del contrato, igual que `DirectThreatMessage`/`DamageRequestMessage`) | Combate lo emite con `MessageWriter`; Movement deriva `LocomotionConstraintFacts` por actor (expiran por silencio — el emisor re-emite cada tick mientras dure el compromiso) y sus motores se abstienen (`sprint::propose` bajo `forbid_sprint`). `Interrupt(Stagger)` llega con `combat-defense`. Combate nunca escribe el `ProposalBuffer` de Movement. |
| `MeleeHitMessage` | `combat/motors/attack.rs` | Interno de Combate: candidato barrido pendiente de resolución (daño + aggro), del sweep a `resolve_melee_hits`. |
| `HitImpactMessage` | `combat/motors/attack.rs` | Golpe resuelto, publicado para presentación (`presentation/juice.rs`: flash, burst, texto de daño, shake, hitstop — ticket `combat-game-feel`). Combate es dueño del tipo y no sabe quién lo consume. Sin campo `attacker` hasta que un consumidor lo lea. |
| `movement::BodyImpulseMessage` (no propio) | ver `movement.md` | Knockback: Combate lo emite al conectar (empujón planar, más fuerte en crítico); Movement lo suma a `BodyVelocity` y el motor activo lo reabsorbe. |
| `health::DamageRequestMessage` (no propio) | ver `health.md` | Combate calcula el monto final (base × mult del paso × bonus de sigilo) y lo emite hacia el `target`; Health valida/aplica, Combate no conoce el pool de vida. |
| `enemies::DirectThreatMessage` (no propio) | ver `enemies.md` | Combate lo emite hacia el enemigo golpeado: aggro instantáneo aunque no te haya visto. |
| `projectiles::SpawnProjectileMessage` (no propio) | ver `projectiles.md` | Combate lo emite al soltar el arco; Projectiles construye y simula la flecha. |
| `inventory::ApplyDurabilityLossMessage` (no propio) | ver `inventory.md` | Combate lo emite al conectar un golpe con el arma equipada; Inventory valida y descuenta `Durability`, Combate no conoce el pool. |

## Estados (`CombatState`)

Implementados: `Idle`, `Windup`, `Active` (hitbox vivo), `Recovery`,
`Aiming` (arco tensado — motor `aim`, ticket `combat-bow`). Por fase:
`Charging` (attack sostenido, drena stamina — `combat-weapon-classes`),
`Guarding`/`Parrying`/`Staggered` (`combat-defense`). El enum crece con cada
fase — agregar un variant no compila hasta escribir su brazo del dispatcher.

Nota clave del modelo: **el número de paso del combo NO vive en el enum** —
`Windup/Active/Recovery` se repiten para cada paso y `ComboLocal.step` dice
cuál es. Mismo criterio que Movement: el enum garantiza exclusividad de
*fase*, el estado propio del motor vive en su componente (ver
`rationale/combat-combo-chains.md`).

Abierto: sub-estados de `Aiming` (tensando vs sostenido) para la cámara
lenta del arco — se decide con Projectiles.

## El modelo de combos por arma

Ver `rationale/combat-combo-chains.md` para el porqué; el qué:

1. Cada `WeaponProfile` trae su cadena: p. ej. la espada de una mano
   `[corte_h, corte_v, estocada]` — pasos cortos, ventanas generosas; la de
   dos manos `[barrido, barrido, giro_360]` — windups largos, más daño, más
   `motion`. La lanza: alcance largo, arcos angostos.
2. Un ataque con `CombatState::Idle` (o encadenado en ventana) entra a
   `Windup` del paso `ComboLocal.step`. Fases avanzan por timers del
   `AttackStep`: `Windup → Active → Recovery`.
3. **Encadenar:** un `attack.pressed` durante `Active`/`Recovery` se
   **bufferea** (mismo patrón que el jump buffer de Movement). Al entrar en
   `Recovery`, si hay buffer y `phase_elapsed < chain_window_secs` →
   `Windup` del paso siguiente. Ventana vencida o cadena completa →
   `Idle` y `step = 0`.
4. **Cargado:** `attack.held` sostenido desde `Idle` → `Charging` (drena
   stamina); al soltar → el `charged` step del arma. Interrumpible por
   `Staggered`.
5. El `motion` del paso se pide a Movement **como intent/constraint, nunca
   escribiendo el cuerpo**: el lunge de una estocada es una propuesta que
   Movement puede rechazar si el suelo no está (mismo principio que todo el
   proyecto). Fase 1: sin `motion` (golpes en el lugar); se agrega cuando el
   feeling lo pida.

## Detección de golpes

Durante `Active`, un sweep por tick: shape intersection (cápsula/arco
posicionado desde el transform del actor + `reach`/`arc_deg` del paso),
**enmascarado a `GameLayer::Actor`** y excluyéndose a sí mismo — la
inversión exacta del sensing de Movement (que enmascara a `Default`): los
golpes solo ven cuerpos, los sensores solo ven mundo. `ActiveSwing` dedupea
por objetivo. Por cada golpe conectado, Combate emite:

1. `health::DamageRequestMessage` (monto final, con bonus de sigilo si
   `!target.Awareness.is_alerted()` — y sneakstrike si además el atacante
   está en `Sneak`),
2. `enemies::DirectThreatMessage` (el golpeado se entera aunque no te vea),
3. `inventory::ApplyDurabilityLossMessage`.

Hasta que Health exista, (1) se sustituye por un cue de debug
(`[combat] hit <target> for <n>`) — patrón GDD §6 de audio diferido — y
(2) ya funciona hoy: pegarle a un bokobo lo agroa de verdad.

## Sistemas (comportamiento)

Pipeline hermano de Movement, `SystemSet`s propios (`CombatSet`),
encadenados en `FixedUpdate` **después de `MovementSet::TickActiveMotor`**
(los hitboxes deben barrer transforms post-movimiento de este mismo tick;
es una config de schedule en un solo punto, no conocimiento del interior de
Movement):

0. **ReadIntents** — un Brain propio de Combate escribe `CombatIntents`,
   mismo slot conceptual que `movement::brain::read_intents`. Lee
   `input::ActiveActions` para el `InputSource` enlazado por
   `input::InputControlledBy` (`input.md`); gatillos con
   `input::InputConsumeCursor` propio. Combate no lee
   `ButtonInput<KeyCode>` ni muta el snapshot global. `EnemyBrain` escribe
   el mismo `CombatIntents` para actores IA (`enemies.md`).
1. **GatherProposals** — motores de combate (`attack`, `guard`, `parry`,
   `aim`, `stagger`) proponen a `CombatProposalBuffer`. `idle::propose`
   empuja `CombatState::Idle` a `Priority::Default` cada frame — análogo al
   `fall::propose` de Movement. El motor `attack` es dueño de las fases
   `Windup/Active/Recovery/Charging` y del avance de `ComboLocal`.
2. **Arbitrate** — núcleo compartido
   (`rationale/proposal-arbitration-core.md`), único escritor de
   `CombatState`.
3. **TickActiveMotor** — **desde el día uno un único dispatcher con `match`
   exhaustivo** (`combat::motors::tick_active_motor`), la lección ya
   aprendida en Movement (`rationale/multi-actor-dispatch.md`): un estado
   nuevo no compila sin su brazo. El reloj de fase (`tick_phase_clock`)
   corre para toda fila. **El sweep de `Active` es un sistema aparte**
   (`sweep_active_swings`, encadenado tras el dispatcher): necesita leer
   transforms de *otros* actores y la query mutable del dispatcher no puede
   aliasar eso. `resolve_melee_hits` corre en `EmitConstraints` y convierte
   candidatos en consecuencias (daño + aggro).
4. **EmitConstraints** — tras `Arbitrate`, traduce ciertos `CombatState` a
   `LocomotionConstraintMessage` hacia Movement: **`Windup`, `Active`,
   `Recovery`, `Charging`, `Guarding`, `Parrying`, `Aiming` →
   `ForbidSprint`** (cualquier compromiso activo con una acción),
   `Staggered` → `Interrupt`. **`Sneak` nunca se restringe** — sigilo +
   ataque es el combo de bonus de daño del GDD (§7), no un conflicto.

   Por qué se generalizó más allá de `Aiming`: `sprint::propose` en
   Movement no exige dirección planar distinta de cero (alcanza con
   `grounded && GaitIntent::Sprint`), así que sostener el modifier de
   esprintar mientras se ataca puede ganarle a `Walk` en arbitración aunque
   el jugador esté quieto atacando — sin la restricción, `LocomotionState`
   puede terminar en `Sprint` con `CombatState::Active`, una combinación
   sin sentido físico.

Qué dispara `Staggered`: un sistema de Combate escucha
`health::DamageAppliedMessage`/`DeathMessage` dirigidos a su propio actor y,
si corresponde, empuja una propuesta `Staggered` con prioridad alta — Health
nunca elige estados de Combate
(`rationale/health-ownership-boundary.md`).

## Relaciones con otros sistemas

| Relación | Quién lee a quién | Mecanismo |
|---|---|---|
| Sigilo: bonus de daño contra un enemigo **no alertado** (flechas y sneakstrike) | Combate lee Enemies (y Movement) | Query read-only sobre `enemies::Awareness` del objetivo: `is_alerted()` = full threat, sin bonus posible — la conciencia del enemigo manda, no el ángulo del golpe. El sneakstrike cuerpo a cuerpo exige además atacante en `Movement::LocomotionState == Sneak`; la flecha solo exige objetivo no alertado. Diseño fijado en ticket `enemy-awareness` (2026-07-15) |
| Golpe conectado agroa al objetivo | Combate → Enemies | `enemies::DirectThreatMessage` (Enemies es dueño del tipo) |
| Flecha del arco: `CombatState::Active` tras `Aiming` | Combate → Projectiles | `projectiles::SpawnProjectileMessage`; Projectiles simula el vuelo y aplica el daño al impactar |
| Compromiso activo impide Sprint | Combate → Movement | `LocomotionConstraintMessage::ForbidSprint(Entity)` — Movement decide el estado locomotor final |
| Recibir un golpe (`Staggered`) interrumpe el motor activo de Movement | Combate → Movement, forzado | `LocomotionConstraintMessage::Interrupt(Entity, InterruptKind::Stagger)` |
| Golpe conectado descuenta durabilidad | Combate → Inventory | `inventory::ApplyDurabilityLossMessage`; Inventory decide si el arma se rompe |
| Hitbox del golpe ve solo cuerpos | Combate lee `world::GameLayer` | Sweep enmascarado a `Actor` (inverso del sensing de Movement) |

Ninguna relación implica que Combate o Movement compartan un `Intents` o un
enum de estado — cada uno tiene el suyo (`rationale/when-not-broker-pattern.md`).

**Timing de `LocomotionConstraintMessage` (decidido):** Movement lo consume
en su `GatherProposals` del frame siguiente — 1 frame de latencia (~16 ms),
imperceptible, y mantiene a Combate ignorante del ordenamiento interno de
Movement. Movement es el único que elige el `LocomotionState` final.

## VFX de swing (placeholder pre-animación) — implementado

Mientras no haya animaciones de combate, el golpe se **lee** con un sector
de arco translúcido (~0.16 s) que aparece al entrar en `Active`, con la
geometría real del paso (`reach`/`arc_deg`). **Cómo quedó implementado**
(divergencia documentada del plan original): `visuals::spawn_swing_vfx`
observa `Changed<CombatState>` read-only — el mismo patrón que el tint de
alerta del bokobo — en lugar de un `CueMessage`, porque el cue actual no
lleva payload posicional/geométrico. Cuando VFX/SFX reales necesiten el
canal de cues, se extiende `CueMessage` con payload y este sistema migra;
Combate no se toca en ningún caso.

## Plan de fases (tickets, en orden)

1. **`combat-scaffolding`** ✅ — plugin, `CombatState`, `CombatIntents`,
   `CombatProposalBuffer`, dispatcher exhaustivo, `idle::propose`, brain de
   hardware, `EmitConstraints` con `ForbidSprint`. Validó que el núcleo de
   arbitraje compartido sirve para Combate sin cambios (la apuesta de
   `proposal-arbitration-core.md`).
2. **`combat-melee-combo`** ✅ (pendiente checkpoint jugado) —
   `WeaponProfile::GRAYBOX_SWORD` (3 pasos), motor `attack` con fases +
   buffer + ventanas de encadenado, sweep de hitbox, VFX de swing, daño como
   cue de log, `DirectThreatMessage` al bokobo. **Primer checkpoint de
   feeling**: pegarle al bokobo, que se agroe, y que el combo "pese" bien.
3. **`health-core`** ✅ (2026-07-16, pendiente checkpoint jugado) — el
   sistema Health (`health.md`): `Health`, `DamageRequestMessage` →
   `DamageAppliedMessage`/`DeathMessage`. Espada y flechas hacen daño real
   (los cues de log placeholder murieron); bokobo/targets mueren →
   despawn; player muere → respawn. Ver ticket `health-core`.
4. **`enemies-combat`** ✅ (2026-07-16, pendiente checkpoint jugado,
   ampliado por pedido del usuario: espada **y arco**) — `EnemyBrain`
   escribe `CombatIntents` (y el arquero su `ControlOrientation`), estrena
   `EnemyAiState::Combat` (`enemies.md`), `WeaponProfile::BOKOBO_CLUB`, y
   un bokobo arquero que carga y suelta con el mismo motor `aim` del
   jugador. Ver ticket `enemies-combat`. La defensa ya tiene contra qué
   existir.
5. **`combat-defense`** — escudo: `Guarding` (mitiga/bloquea) y `Parrying`
   (ventana temporizada); `Staggered` entrante desde
   `DamageAppliedMessage`.
6. **`combat-bow`** ✅ (adelantado, pendiente checkpoint jugado) — `Aiming`
   (mouse derecho) + motor `aim` (soltar = silencio → Idle; click = flecha
   por `SpawnProjectileMessage` en la dirección de `ControlOrientation`),
   Projectiles con vuelo parabólico y bonus ×4 sobre no-alertados, cámara
   over-shoulder con mira, y 3 targets de práctica en capa `Actor` en el
   graybox (las queries de target del melee pasaron a ser layer-gated, no
   marker-gated). Ver ticket `combat-bow`.
7. **`camera-lock-on`** — el lock-on vive en Camera (`camera.md`), no en
   Combate: selección del objetivo (criterio en COUPLING-MAP, abierto →
   se decide ahí), órbita fijada al objetivo, y strafe: con lock activo el
   brain de Movement mapea input planar relativo al objetivo. Combate solo
   expone read-only qué actores son objetivos válidos (`Enemy` + vivo).

## Decisiones abiertas

- Valores iniciales de la cadena de la espada (duraciones/mults) — se fijan
  jugando el checkpoint de la fase 2, no antes (Constitución §10/§11).
- Arco: mecánica exacta de apuntado (cámara lenta, primera persona) — GDD
  §8, con Projectiles (fase 6).
- Cuánta durabilidad descuenta cada tipo de golpe (con Inventory — fuera
  del MVP).
- `motion` de los pasos (lunge de estocada): forma exacta del pedido a
  Movement — ¿intent efímero o constraint con vector? Se decide cuando el
  feeling de la fase 2 lo pida.
- Criterio de selección de lock-on (Camera↔Enemies, ya listado en
  COUPLING-MAP) — se decide en la fase 7.
- Aggro por daño sin visión: `DirectThreatMessage` deja al enemigo
  `ALERTED`, pero `next_ai_state` exige `visible && alerted` para `Alert` —
  un bokobo flechado por la espalda va a `Search` y **camina** a investigar
  en vez de perseguir. ¿Es el feeling correcto o un golpe directo debería
  habilitar la persecución hacia `last_seen` aunque no haya visión? Se
  decide jugando en `enemies-combat`.
- Daño a `NonClimbable`/mundo (¿cortar pasto?): fuera de alcance hasta que
  World tenga objetos rompibles.

**Descartado** (2026-07-15): flurry rush / perfect dodge con dilatación de
tiempo; clases `TwoHanded`/`Spear` dentro del MVP (el modelo de datos las
soporta; se agregan como presets cuando haga falta variedad).
