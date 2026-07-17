# Auditoría adversarial de arquitectura ECS — 2026-07-17

**Alcance:** exclusivamente `src/` (no se leyó `docs/`, `AGENTS.md` ni ningún `.md` del repo).

**Premisa:** se asume que la arquitectura tiene errores estructurales y el objetivo de esta
auditoría es encontrarlos, no validar el diseño existente. Bevy 0.19 + avian3d, ~14-19k
líneas en `src/`.

**Método:** lectura directa de módulos (movement, combat, mounts, enemies, health, input,
presentation, projectiles, sfx, camera, world, player, visuals, debug) y de los `Plugin::build`
de cada uno para reconstruir schedules, sets, ordering y fronteras. Cada hallazgo se agrega
apenas se confirma con Read/grep — no se pospone al final.

---

## Hallazgos

(se agregan incrementalmente abajo)

### H1 — Preparación de capacidad en `Update`, consumo en `FixedUpdate` (patrón repetido)

**Severidad:** alto

**Archivos:línea:**
- `src/movement/mod.rs:105` — `app.add_systems(Update, attachment_systems::prepare_actor_link_workspace);`
- `src/movement/mod.rs:142` (dentro del bloque `FixedUpdate` en `MovementSet::ApplyExternal`) — `attachment_systems::apply_actor_link_requests` es el consumidor de `ActorLinkWorkspace.prepared_for` (ver `src/movement/attachment_systems.rs:19-56`).
- `src/mounts/mod.rs:35` — `app.add_systems(Update, charge::prepare_hit_ledger);`
- `src/mounts/mod.rs:94` — `charge::detect_charge_hits.in_set(MountsSet::Charge)` (registrado en `FixedUpdate`), consumidor de `ChargeHitLedger` preparado por `prepare_hit_ledger` (ver `src/mounts/charge.rs:58-70` y `27-33`).

**Claim arquitectónico:** el mismo patrón aparece dos veces: un sistema "preparador" que fija una capacidad/snapshot de un `Resource` (`ActorLinkWorkspace.prepared_for`, `ChargeHitLedger` capacity) corre en el schedule `Update` (una vez por frame real), mientras el sistema que efectivamente lee/consume y aplica ese estado corre en `FixedUpdate` (que puede ejecutarse 0, 1 o N veces por frame real, según el acumulador de `Time<Fixed>` a 60 Hz). No hay ninguna relación `.before()`/`.after()` ni sets compartidos entre el preparador (`Update`) y el consumidor (`FixedUpdate`) — dependen implícitamente de que ambos schedules corran "una vez por frame", suposición que Bevy no garantiza.

**Consecuencia práctica:** en un frame con catch-up (hitch, VSync, carga), `FixedUpdate` puede correr 2+ veces seguidas sin que `Update` corra entre medio: el snapshot de capacidad queda desactualizado para el segundo/tercer tick del mismo frame. El propio código ya anticipa el síntoma con el mecanismo `CapacityPending`/retry (`src/movement/attachment_systems.rs:207-216`, warn! en `src/mounts/charge.rs:213-215` "Update will reserve it") — es decir, la arquitectura sabe que puede desincronizarse y compensa con reintentos de un frame en vez de eliminar la causa (calcular la capacidad dentro del mismo `FixedUpdate`, antes del consumidor). Al crecer el juego (más jinetes, más enemigos, más actores concurrentes), la ventana de inconsistencia crece proporcional al número de fixed-ticks de catch-up, y cada nuevo sistema que copie este patrón (ya van dos) repite el mismo defecto de diseño en vez de centralizarlo.

**Cómo verificarlo:** grep de `add_systems(Update,` vs `add_systems(FixedUpdate,` para `prepare_actor_link_workspace`/`apply_actor_link_requests` y `prepare_hit_ledger`/`detect_charge_hits`; revisar que `ActorLinkWorkspace`/`ChargeHitLedger` no tengan ninguna dependencia de orden cross-schedule (Bevy no permite `.before()/.after()` entre schedules distintos, por lo que la relación es puramente temporal/implícita).

**Mejora propuesta:** mover ambos preparadores a `FixedUpdate`, encadenados con `.before(...)` del consumidor real dentro del mismo set (p. ej. `MovementSet::ApplyExternal` y `MountsSet::Charge` respectivamente), eliminando la necesidad de "capacidad preparada de antemano" cruzando schedules. Si la única razón de estar en `Update` es evitar recalcular en cada fixed-tick, cachear el conteo en el propio `FixedUpdate` con un `Local<usize>` o similar, nunca dependiendo de que `Update` corra exactamente una vez por fixed-tick.

---

### H2 — `ControlOrientation` (mouse-look) se escribe en `Update` y la simulación de `FixedUpdate` la lee como si fuera determinista

**Severidad:** crítico

**Archivos:línea:**
- `src/input/mod.rs:38-43` — `app.add_systems(Update, (cursor_control, update_local_orientation).chain().in_set(InputSet::UpdateOrientation));`
- `src/input/mod.rs:145-161` — `update_local_orientation` escribe `ControlOrientation.yaw/pitch` usando `AccumulatedMouseMotion.delta` sin escalar por `dt` fijo, una vez por frame de render.
- `src/movement/brain.rs:23` (campo de `BrainQuery`) y `src/movement/brain.rs:32,73-76` — `read_intents` (registrado en `FixedUpdate` / `MovementSet::ReadIntents`, ver `src/movement/mod.rs:135-138`) lee `orientation.yaw` para rotar el input plano a espacio mundo (`Quat::from_rotation_y(orientation.yaw)`), es decir, la dirección de movimiento del jugador depende de este valor.
- `src/combat/motors/aim.rs:163,184` — `ShootQuery`/`shoot_drawn_arrow` (registrado en `FixedUpdate` / `CombatSet::GatherProposals`, ver `src/combat/mod.rs:64-74`) también toma `&ControlOrientation` para calcular la dirección de disparo del arco.

**Claim arquitectónico:** `ControlOrientation` es, por comentario del propio archivo (`src/input/frame.rs:27`, "owned by input/control, not presentation"), estado de **simulación**, no de presentación — y de hecho alimenta tanto la dirección de movimiento (`movement::brain::read_intents`) como la dirección de disparo del arco (`combat::motors::aim::shoot_drawn_arrow`), ambos sistemas pinneados a `FixedUpdate` a 60 Hz. Sin embargo el único escritor para el jugador local, `update_local_orientation`, corre en `Update` (framerate variable, no determinista), acumulando el delta del mouse de ese frame de render sin ninguna normalización por tiempo fijo. En Bevy 0.19 el bucle de `FixedMain` (que contiene `FixedUpdate`) se ejecuta dentro de `PreUpdate`/`RunFixedMainLoop`, **antes** de `Update` en el mismo frame — por lo tanto los ticks de `FixedUpdate` de un frame dado consumen el `ControlOrientation` tal como quedó al final del `Update` del frame *anterior*: hay un frame completo de retraso estructural, no ocasional. Además, cuando el framerate cae y `FixedUpdate` corre 2+ sub-pasos para ponerse al día (catch-up), todos esos sub-pasos leen exactamente el mismo `ControlOrientation` (porque `Update` — su único escritor — corrió una sola vez), de modo que la dirección de movimiento y de puntería queda "congelada" durante varios ticks de física y luego salta de golpe cuando `Update` vuelve a correr.

**Consecuencia práctica:** el giro de cámara del jugador (mouse-look) introduce lag estructural y jitter dependiente del framerate exactamente en los dos sistemas que más se benefician de precisión determinista: dirección de movimiento y puntería del arco (el propio commit reciente "industry-standard two-phase bow aim" sugiere que la precisión del apuntado ya es un problema conocido). Cuantos más actores/mecánicas dependan de orientación de cámara (mounts, futuras armas a distancia, red multiplayer), más se propaga esta inconsistencia: cualquier sistema nuevo en `FixedUpdate` que lea `ControlOrientation` hereda el mismo desfase sin que nada en el tipo o en el schedule lo señale.

**Cómo verificarlo:** `grep -n "ControlOrientation" src/input/mod.rs src/movement/brain.rs src/combat/motors/aim.rs`; confirmar el registro de schedules en `src/input/mod.rs:38-43`, `src/movement/mod.rs:133-138`, `src/combat/mod.rs:59-74`. El orden `PreUpdate → RunFixedMainLoop(FixedUpdate…) → Update` es el orden estándar de `Main` en Bevy ≥0.12/0.19.

**Mejora propuesta:** mover la resolución de `ControlOrientation` a `PreUpdate` (junto a `resolve_local_actions`, que ya corre ahí y sí precede a `FixedUpdate` en el mismo frame), o — mejor para determinismo real — acumular el delta del mouse en un buffer leído y consumido dentro de `FixedUpdate` (drenando `AccumulatedMouseMotion` una vez por tick fijo en vez de una vez por frame de render), de forma que cada sub-paso de física/combate vea una orientación distinta y correctamente proporcional a su propio `dt` fijo en vez de una copia congelada del frame anterior.

---

### H3 — `enemies::perception::perceive` hardcodea `With<Player>` en vez de `With<Actor>`, rompiendo la premisa multi-actor que el propio módulo Movement se esfuerza en garantizar

**Severidad:** medio

**Archivo:línea:** `src/enemies/perception.rs:188` — `targets: Query<TargetQuery, (With<Player>, With<Actor>)>` dentro de `perceive` (registrada en `FixedUpdate`/`MovementSet::ReadIntents` vía `src/enemies/mod.rs:66-78`).

**Claim arquitectónico:** todo el módulo `movement` está construido, con tests dedicados (`actor_isolation_tests`, `spike.rs`), sobre la premisa de que "Motors dispatch on `Actor`, not `Player`" (comentario en `src/movement/mod.rs:50-53`): cualquier entidad con `Actor` — jugador, jugador remoto, montura, otro NPC — debe poder participar en la simulación sin casos especiales. `enemies::perception::perceive`, sin embargo, filtra explícitamente por `With<Player>` para decidir a quién puede ver/oír un enemigo, en vez de `With<Actor>` (o `With<Actor>, Without<Enemy>`). Esto acopla el único sistema de IA "sensorial" del juego a la existencia de exactamente un jugador local, contradiciendo la premisa multi-actor que el resto de la base de código proclama activamente sostener.

**Consecuencia práctica:** hoy, con un solo jugador, no se manifiesta como bug. Pero apenas el juego crezca a lo que la arquitectura de Movement ya soporta — un segundo jugador (co-op/red), un aliado/NPC no-enemigo, o incluso enemigos que deberían percibirse entre sí — `perceive` los ignora silenciosamente: ningún enemigo reaccionará jamás a nada que no sea `Player`. Es el tipo de suposición de singleton que, al escalar a N actores, exige reescribir el sistema entero en vez de extenderlo, justo lo que el resto del código (Broker, `Actor` genérico, tests de aislamiento) fue diseñado para evitar.

**Cómo verificarlo:** `grep -n "With<Player>, With<Actor>" src/enemies/perception.rs`; comparar con el resto de queries de IA en el mismo archivo/módulo, todas ellas ya usan `With<Enemy>` para el propio actor pero no distinguen entre "objetivo = Player" y "objetivo = cualquier Actor perceptible".

**Mejora propuesta:** cambiar el filtro de objetivos a `With<Actor>` (excluyendo `With<Enemy>` si no se quiere que los bokobos se perciban entre sí todavía), y si se necesita distinguir facciones/hostilidad, introducir un componente explícito de facción en vez de reutilizar el marcador `Player` como proxy de "objetivo percibible".

---

### H4 — Latencia de un tick entre `CombatSet::EmitConstraints` y el consumo del mensaje en `MovementSet` (auto-reconocida en el propio código, no corregida)

**Severidad:** bajo

**Archivo:línea:** `src/combat/mod.rs:102-114` — `emit_constraints` escribe `LocomotionConstraintMessage::ForbidSprint` en `CombatSet::EmitConstraints`, comentado explícitamente como "Movement consumes the message in its own frame (1 tick later, accepted...)"; el consumidor, `movement::constraints::apply_locomotion_constraints`, corre en `src/movement/mod.rs:106-114`, en el bloque `.after(MovementSet::SenseWorld).before(MovementSet::GatherProposals)` — es decir, **antes** de que Combat corra ese mismo tick (`CombatSet` entero está `.after(MovementSet::TickActiveMotor)`, ver `src/combat/mod.rs:45-57`).

**Claim arquitectónico:** cada actor comprometido en combate (`state.commits_the_body()`) debería perder la capacidad de esprintar en el mismo tick en que se compromete, pero el mensaje que expresa esa restricción solo puede ser leído por Movement en el tick siguiente, porque Combat corre estrictamente después de todo el pipeline de Movement dentro del mismo tick fijo. El propio comentario del código admite la ventana de un tick como "accepted", pero no hay ninguna guarda para el caso en que el consumidor (`apply_locomotion_constraints`) no vacíe el buffer de mensajes cada tick — si Movement se salta un tick (raro, pero ver H1 sobre desincronía Update/FixedUpdate) la ventana de inconsistencia podría ampliarse más allá de "1 tick".

**Consecuencia práctica:** a 60 Hz, ~16 ms de ventana en la que un actor recién comprometido a un ataque todavía puede iniciar un sprint; probablemente imperceptible hoy, pero es exactamente el tipo de acoplamiento entre dos Broker paralelos (Movement/Combat) que se vuelve más frágil si en el futuro se añade un tercer Broker (p. ej. mounts) que también necesite vetar/forzar estado de locomoción en el mismo tick en que se decide.

**Cómo verificarlo:** confirmar el orden de sets con `grep -n "configure_sets" src/movement/mod.rs src/combat/mod.rs` y el comentario textual en `src/combat/mod.rs:102-104`.

**Mejora propuesta:** si el veto de un tick de latencia es aceptable, dejarlo — pero documentarlo con un test de integración explícito (`app.update()` dos veces, comprobando el sprint se bloquea en t+1, no en t) para que un futuro refactor de ordering no lo rompa silenciosamente sin que ningún test lo note.

---

## Bugs de lógica obvios encontrados de pasada

Ninguno detectado que amerite reportarse aparte — el código revisado (movement, combat, mounts, health, enemies, input, presentation, proposal) está cubierto por tests unitarios relativamente exhaustivos y no se encontraron contradicciones obvias de lógica de gameplay durante esta auditoría (que se centró en arquitectura, no en gameplay).

## Ranking final

1. **H2** (crítico) — `ControlOrientation` escrito en `Update`, leído por movimiento/puntería en `FixedUpdate`: framerate-dependence estructural en las dos mecánicas que más necesitan determinismo.
2. **H1** (alto) — patrón repetido de preparar capacidad de un `Resource` en `Update` y consumirla en `FixedUpdate` (`ActorLinkWorkspace`, `ChargeHitLedger`), con mecanismo de reintento que reconoce el problema en vez de eliminarlo.
3. **H3** (medio) — `enemies::perception::perceive` hardcodea `With<Player>` en vez de `With<Actor>`, rompiendo la premisa multi-actor que el resto de la base sostiene activamente.
4. **H4** (bajo) — latencia de un tick entre el veto de combate (`ForbidSprint`) y su aplicación en Movement, auto-reconocida en el código pero sin test de regresión que la fije.

