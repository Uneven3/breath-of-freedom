# Rationale: dispatch multi-actor (`Actor` genérico)

Monturas, Enemies y Multiplayer necesitan N actores corriendo el mismo
pipeline (Brain → Intents → Broker → Motors → Body) simultáneamente e
independientemente. El contrato objetivo es un marker genérico `Actor`:
`Player`, `Enemy`, jugadores remotos y otros cuerpos controlables son
especializaciones de ese actor.

## Decisión

El pipeline de simulación no debe depender de `Single<..., With<Player>>`.
Los sistemas de Movement y Combat operan sobre `Query<..., With<Actor>>`,
iterando todos los actores relevantes.

Los run conditions globales por estado locomotor se reemplazan por guards
internos por entidad:

```rust
if *state != LocomotionState::X {
    continue;
}
```

Todo estado temporal que pueda variar por actor vive en componentes por
entidad, no en `Local<T>` compartido por el sistema.

## Por qué

- **Enemies:** `EnemyBrain` necesita escribir `Intents`/`CombatIntents` en
  entidades que no son el jugador local.
- **Multiplayer:** un actor remoto es un `Actor` cuyo `InputSource` viene de
  red; los mismos Brains genéricos traducen `ActiveActions` a
  `Intents`/`CombatIntents`.
- **Monturas:** el pipeline de Mounts es separado, pero jinetes no-jugador
  heredan el mismo modelo de actor genérico.

Ver `docs/architecture/movement.md` y `docs/ARCHITECTURE-MAP.md`
(categoría `BLOCKING-PREREQUISITE`).
