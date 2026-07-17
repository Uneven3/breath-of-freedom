# Validación del audit adversarial de arquitectura — 2026-07-17

Análisis de veracidad de los 4 hallazgos de `adversarial-arch-audit-2026-07-17.md`,
verificado contra el código real y contra el fuente de Bevy 0.19 en el registry local.

**Resultado global: 0 alucinaciones.** Toda cita `archivo:línea` del audit existe
textualmente. Los 4 hallazgos son reales como observaciones; difieren en severidad
práctica y en cuánto contexto de diseño les faltó al auditor (que por regla no leyó
docs/). Detalle:

---

## H2 — ControlOrientation escrito en Update, leído en FixedUpdate

**Veredicto: REAL — el hallazgo más valioso del audit.**

Verificado:
- `update_local_orientation` corre en `Update` (`src/input/mod.rs:38-43`) ✓
- `brain::read_intents` y `aim::shoot_drawn_arrow` lo consumen en `FixedUpdate` ✓
- Orden de `Main` confirmado en `bevy_app-0.19.0/src/main_schedule.rs` (`MainScheduleOrder::default`):
  `First → PreUpdate → RunFixedMainLoop → Update → …` — FixedUpdate corre **antes**
  que Update en el mismo frame, así que la simulación consume la orientación del
  frame anterior. Un frame completo de latencia estructural en dirección de
  movimiento y puntería. ✓
- Agravante que el audit detectó bien: `resolve_local_actions` (botones) SÍ corre en
  `PreUpdate` (`src/input/mod.rs:37`) — el propio módulo ya conoce el lugar correcto
  y quedó asimétrico: el salto del jugador es del frame actual, su dirección de
  mirada es del frame anterior.

Matices (errores menores del auditor, no invalidan el claim):
- "sin escalar por dt fijo" es una crítica incorrecta: el delta de mouse es
  desplazamiento posicional, no una tasa — no se escala por dt en ningún motor.
- El "congelamiento durante catch-up" es real pero inherente a muestrear input una
  vez por frame de render; mover el sistema no lo elimina (con N sub-pasos y una
  muestra de mouse, N-1 sub-pasos verán lo mismo hagas lo que hagas).
- Severidad "crítico" es exagerada para ~16 ms de lag a 60 fps; "alto" es lo justo
  dado que golpea exactamente la puntería (el sistema que más se ha iterado).

**Fix recomendado (barato):** mover `(cursor_control, update_local_orientation)` de
`Update` a `PreUpdate` (encadenado después de `resolve_local_actions`), restaurando
la simetría botones/orientación. Nota: la cámara (`follow_player`, en `Update`) debe
seguir leyendo la orientación después — hoy ya corre en `Update`, así que no cambia.

---

## H1 — Preparar capacidad en Update, consumir en FixedUpdate (ActorLinkWorkspace, ChargeHitLedger)

**Veredicto: REAL como patrón, severidad inflada (alto → medio).**

Verificado:
- `prepare_actor_link_workspace` en `Update` (`src/movement/mod.rs:105`), consumidor
  en `FixedUpdate/ApplyExternal` ✓
- `prepare_hit_ledger` en `Update` (`src/mounts/mod.rs:35`), consumidor
  `detect_charge_hits` en `FixedUpdate/MountsSet::Charge` ✓
- Retry `CapacityPending` (`attachment_systems.rs:207-216`) y el
  `warn!("… Update will reserve it")` (`charge.rs:213-215`) existen ✓

Lo que el auditor no vio (no leyó docs/ por regla): el patrón es deliberado — su
propósito es mantener `FixedUpdate` libre de allocations (los `reserve()` viven en
Update). Y el retry no pierde requests: `retry_capacity_pending` re-escribe el
mensaje cada tick hasta que la capacidad alcanza, así que es eventualmente
consistente — no hay bug de correctness, hay latencia extra (≥1 frame) en el peor
caso y complejidad cross-schedule real.

**Juicio:** el costo (dos mecanismos de retry, un warn en runtime, acoplamiento
temporal implícito entre schedules) no justifica el beneficio (evitar un `reserve()`
amortizado O(1) por tick). La propuesta del audit es correcta: calcular capacidad
dentro de `FixedUpdate` antes del consumidor y borrar el patrón entero, incluido el
estado `CapacityPending`. Es una simplificación, no un fix urgente.

---

## H3 — perceive filtra With<Player> en vez de With<Actor>

**Veredicto: REAL como límite de escalado; NO es un error activo — es alcance actual.**

Verificado: `targets: Query<TargetQuery, (With<Player>, With<Actor>)>`
(`src/enemies/perception.rs:188`) ✓

El propio audit lo admite: "hoy, con un solo jugador, no se manifiesta como bug".
Es una decisión de alcance (los enemigos cazan al jugador, fase actual del juego)
que contradice la premisa multi-actor del resto del código. Se vuelve deuda real
en cuanto exista algo más que los enemigos deban percibir — y ya existe un candidato:
el **horse** es un Actor que plausiblemente debería ser atacable/percibible.

**Juicio:** anotarlo como deuda con dirección clara (componente de facción o marcador
`Perceivable` en vez de `Player` como proxy), y pagarla cuando el gameplay lo pida,
no antes. Severidad "medio" es razonable.

---

## H4 — Latencia de 1 tick del veto ForbidSprint

**Veredicto: REAL, ya conocido y aceptado por diseño; el aporte neto es el test faltante.**

Verificado:
- Comentario textual "1 tick later, accepted" (`src/combat/mod.rs:102-104`) ✓
- Orden de sets: `CombatSet::*.after(MovementSet::TickActiveMotor)`
  (`src/combat/mod.rs:56`), y `apply_locomotion_constraints` corre antes de
  `GatherProposals` — el mensaje se lee al tick siguiente ✓

La especulación de que "la ventana podría ampliarse más allá de 1 tick" es débil:
los mensajes de Bevy persisten entre ticks del mismo schedule y el consumidor corre
cada tick fijo; no hay mecanismo realista para perderlos.

**Juicio:** la única acción con valor es la que propone el audit: un test de
integración que fije la ventana en exactamente 1 tick, para que un refactor de
ordering no la agrande sin que nadie lo note.

---

## Ranking accionable (post-validación)

1. **H2** — mover orientación de mouse a `PreUpdate`. Barato, elimina un frame de
   lag en puntería y movimiento, restaura la simetría con los botones. Hacer ya.
2. **H4** — test de regresión de la ventana de 1 tick del veto de sprint. Barato.
3. **H1** — simplificación: capacidad calculada en `FixedUpdate`, borrar
   `CapacityPending` y sus retries. Mediano, sin urgencia.
4. **H3** — deuda anotada; pagar cuando haya un segundo perceptible (horse, aliado,
   co-op).
