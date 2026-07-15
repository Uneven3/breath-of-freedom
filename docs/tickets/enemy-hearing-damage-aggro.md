# Ticket: `enemy-hearing-damage-aggro`

## Sistema(s)

Enemies. Agrega los dos estímulos que faltaban al modelo de sentidos de
`enemy-awareness`: **oído** (área de sonido omnidireccional) y **aggro por
daño** (alerta instantánea). Ambos alimentan el mismo medidor `Awareness`.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/architecture/enemies.md`.
3. `docs/architecture/rationale/enemy-senses.md` (creado por este ticket —
   el modelo completo de sentidos).
4. `docs/tickets/enemy-awareness.md` (el slice que este extiende).

## Acoplamiento

- **Oído — Enemies lee Movement (READ):** la "firma de ruido" del target se
  deriva de su `LocomotionState` + `BodyVelocity`, ambos ya expuestos
  read-only. No se usa `presentation::cues` (son presentación: sin posición,
  derivados para SFX/VFX — alimentar simulación desde ahí invertiría su
  capa). No hay canal de ruido nuevo en Movement.
- **Aggro por daño — patrón mensaje-del-receptor:** `DirectThreatMessage` es
  **propiedad de Enemies**; Health/Combat lo emitirán cuando existan (mismo
  patrón que `health::DamageRequestMessage`, que Combate emitirá sin ser su
  dueño). No se asume la forma de `DamageAppliedMessage` (Health no existe);
  cuando Health llegue, emite este mensaje o un adaptador lo traduce.

## Alcance (File Touches)

- `src/enemies/perception.rs`, `src/enemies/mod.rs`
- `docs/architecture/enemies.md`
- `docs/architecture/rationale/enemy-senses.md` (nuevo)
- `docs/tickets/enemy-hearing-damage-aggro.md` (este archivo)

## Fuera de alcance

- Emisores de `DirectThreatMessage` (Health/Combat no existen). Sin teclas
  de debug que lo emitan: Debug es read-only sobre simulación (regla del
  ticket `traversal-probe`).
- Ruidos discretos del mundo (ollas, silbidos, aterrizajes como pico) — el
  oído continuo por gait cubre el graybox.
- Alerta grupal (`Faction`): un bokobo alertado no avisa a otros todavía.

## Diseño

**Oído** (en `perceive`, rama sin visión):
- Omnidireccional — la espalda no existe para el oído.
- `loudness` del target derivada de su gait, solo si se está moviendo:
  `Sprint 1.0 > Walk 0.55 > Sneak 0.15`; quieto = silencio.
- Radio audible efectivo = `hearing_range × loudness`, atenuado por
  `wall_muffle` si la línea de visión está ocluida (las paredes amortiguan,
  no bloquean).
- Llena `Awareness` **con techo en `SUSPICIOUS`**: un ruido solo nunca da
  full threat — hace girar e investigar; la visión toma el relevo. Si el
  medidor ya está por encima (decayendo de una alerta), el ruido lo
  sostiene, no lo baja.
- Actualiza `last_seen` a la posición del ruido → `decide`/`act` ya saben
  investigar sin cambios.

**Aggro por daño** (`receive_direct_threats`, entre perceive y decide):
- `DirectThreatMessage { enemy, threat_position }` → `Awareness = ALERTED` y
  `last_seen = threat_position`, saltándose el medidor: recibir daño no es
  un estímulo ambiguo.
- Nota: si el atacante sigue fuera de vista, `decide` produce `Search` a
  full awareness (corre a investigar, sin sneakstrike posible para Combate);
  si entra en vista, `Alert` directo. Ambos emergen sin tocar `decide`.

## Definición de terminado

- [x] fmt/clippy/test limpios.
- [x] Invariantes §11: loudness pura testeada (sprint > walk > sneak >
      quieto), techo de sospecha del oído, no-bleed del mensaje entre
      enemigos, el oído no baja un medidor más alto.
- [x] `enemies.md` y `rationale/enemy-senses.md` reflejan el modelo.
- [ ] Checkpoint de *feeling*: esprintar detrás de un bokobo debe girarlo;
      sneak detrás debe seguir siendo seguro.
