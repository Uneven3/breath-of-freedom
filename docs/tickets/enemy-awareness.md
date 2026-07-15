# Ticket: `enemy-awareness`

## Sistema(s)

Enemies. Reemplaza la detección binaria del slice `bokobo-brain` por un
medidor de alerta gradual (`Awareness`), que además **fija el contrato** que
Combate leerá para el bonus de sigilo — sin implementar nada de daño.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — fila `Enemies`.
3. `docs/COUPLING-MAP.md` — Enemies↔Movement, Enemies↔Combat.
4. `docs/architecture/enemies.md`.
5. `docs/architecture/combat.md` § Relaciones (bonus de sigilo).
6. `docs/tickets/bokobo-brain.md` (el slice que este ticket extiende).

## Acoplamiento

- **Enemies↔Combat: Tight, Combat no existe** → este ticket solo construye
  el lado Enemies (`Awareness` + su semántica) y deja el contrato escrito en
  ambos docs. Ningún código de multiplicadores de daño.
- **Enemies↔Movement: READ nuevo** — `perceive` lee el `LocomotionState`
  del target (¿va en Sneak?) para modular la velocidad de detección. Query
  read-only, ya anticipado en `enemies.md`.

## Diseño (del feedback del usuario, 2026-07-15)

- Un enemigo que te ve tiene **full threat**: `Awareness` llena → `Alert`,
  sin bonus de sigilo posible (tampoco "por la espalda": por la espalda no
  te ve, así que el medidor nunca llenó — el ángulo no necesita regla
  propia, lo produce el cono de visión).
- Un enemigo que **no se percata** de ti: flechas y ataques sigilosos harán
  mucho más daño — Combate leerá `Awareness::is_alerted()` del objetivo.
- El medidor llena mientras estás en cono+línea de visión — más rápido de
  cerca, más lento si vas en `Sneak` — y decae al perder la visión.
- Umbral intermedio `SUSPICIOUS`: el enemigo investiga (`Search` hacia el
  estímulo) antes de estar plenamente alertado.

## Alcance (File Touches)

- `src/enemies/perception.rs`, `src/enemies/brain.rs`, `src/enemies/mod.rs`
- `src/visuals.rs` (tinte del bokobo por nivel de alerta — feedback de
  playtest sin UI)
- `docs/architecture/enemies.md`, `docs/architecture/combat.md` (la fila del
  bonus de sigilo pasa a leer `Awareness` del objetivo)
- `docs/tickets/enemy-awareness.md` (este archivo)

## Fuera de alcance

- Daño, multiplicadores, `CombatIntents` (Combat no existe).
- Oído/ruido como estímulo (el sneak modula la *visión* por ahora).
- UI de "¿!"; el tinte de la cápsula es el sustituto de graybox.
- `Faction`/alerta grupal.

## Definición de terminado

- [x] fmt/clippy/test limpios.
- [x] `enemies.md` y `combat.md` reflejan el contrato `Awareness`.
- [x] Invariantes §11: fill-rate puro testeado (cerca > lejos, sneak <
      caminar), transiciones puras con sospecha, clamps del medidor,
      no-bleed entre enemigos se mantiene.
- [ ] Checkpoint de *feeling*: acercarse en sneak por la espalda debe ser
      viable; de frente y corriendo, detección casi inmediata.
