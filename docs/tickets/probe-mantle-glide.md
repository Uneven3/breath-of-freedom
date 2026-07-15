# Ticket: `probe-mantle-glide`

## Sistema(s)

Movement (extensión del `TraversalProbe` del ticket `traversal-probe`). El
escenario de integración crece de "escalar y sostenerse bajo el borde" a la
vuelta completa: **mantle al tope del muro → asentarse → girar 180° → saltar
→ glide hasta el suelo**. Sigue siendo un brain de integración: escribe solo
sus `Intents` y observa el pipeline normal.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/tickets/traversal-probe.md` (el ticket base y sus reglas).
3. `docs/architecture/movement.md` (§ TraversalProbe).

## Acoplamiento

Sin cambios respecto a `traversal-probe`: Movement-interno, ningún tipo de
Enemies, Debug sigue read-only, Input no participa.

## Alcance (File Touches)

- `src/movement/probe.rs`, `src/movement/probe_data.rs`
- `docs/architecture/movement.md` (párrafo del escenario)
- `docs/tickets/probe-mantle-glide.md` (este archivo)

## Fuera de alcance

- Igual que el ticket base: nada de estados forzados, teleports ni escritura
  de `LocomotionState`/`Transform` — cada etapa avanza solo cuando sensores
  y arbitraje reales alcanzan su condición observada.
- No toca motores ni servicios; si una maniobra no sale, eso es un bug de
  Movement que se ve como timeout del probe, no algo que el probe parchea.

## Diseño de las etapas nuevas

`HoldAtLip` deja de ser terminal: tras el settle (que preserva el checkpoint
"sostenerse sin mantle accidental") avanza a:

1. **MantleOntoTop** — `traversal = Mantle` (+ climb sostenido) hasta
   observar `state == Mantle`.
2. **SettleOnTop** — sin input hasta `state == Walk` (aterrizó en el tope).
3. **TurnAround** — planar +Z (de vuelta hacia el lado de spawn) hasta que
   el forward real del cuerpo apunte a +Z (los motores rotan el cuerpo; el
   probe solo observa la rotación).
4. **JumpOff** — planar +Z + jump hasta observar `state == Fall`.
5. **GlideDown** — planar +Z + `GlideIntent::Requested` **solo mientras
   `state ∈ {Fall, Glide}`**: el gate garantiza que el flanco de "pulsación
   fresca" que `glide::propose` exige caiga estando en `Fall` (pedirlo
   durante `Jump` gastaría el flanco un frame antes de tiempo). Completa al
   aterrizar (`Walk`) **habiendo observado `Glide`** (`glide_observed` en el
   script) — aterrizar sin haber planeado no cuenta.

`ProbeCoverage` pasa de `u8` a `u16` (9 etapas > 8 bits).

## Definición de terminado

- [x] fmt/clippy/test limpios.
- [x] Tests puros de la máquina de etapas (avances, gate del glide, giro
      observado por rotación) — invariantes §11.
- [x] `movement.md` refleja el escenario completo.
- [ ] Checkpoint jugado: F6 completa la vuelta entera sin timeouts.
