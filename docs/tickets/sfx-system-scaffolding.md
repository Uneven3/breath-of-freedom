# Ticket: sfx-system-scaffolding

## Sistema(s)
SFX (y el bus de presentación compartido `presentation/cues.rs`).

## Lectura obligatoria, en este orden
1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — fila `SFX`.
3. `docs/COUPLING-MAP.md` — relación `SFX` con otros.
4. `docs/architecture/sfx.md`.
5. `docs/architecture/rationale/presentation-cues.md`.
6. `docs/gdd.md` §6.

## Acoplamiento
- Loose con todos los emisores de cues (Movement, Combat, World).
- Establece el contrato `CueMessage` en `src/presentation/cues.rs` para que otros agentes lo puedan importar.
- Lee `BodyVelocity` (en `src/movement/mod.rs`) y `Stamina` (en `src/movement/stamina.rs`) para demostración de modulación continua.

## Alcance (File Touches)
- `src/presentation/mod.rs`
- `src/presentation/cues.rs`
- `src/sfx/mod.rs`
- `src/main.rs` (para registrar el SFX plugin)
- `docs/tickets/sfx-system-scaffolding.md` (este archivo)

## Fuera de alcance
- No agrega assets reales de audio (wav, ogg) ni integra FMOD/Wwise todavía — es puro scaffolding con logs y modulación de debug como pide el GDD §6.
- No modifica otros sistemas para emitir cues (eso lo harán los respectivos sistemas en sus propios tickets/worktrees, respetando el contrato que definamos aquí).

## Definición de terminado
- [ ] `cargo fmt` limpio.
- [ ] `cargo clippy` sin warnings.
- [ ] `cargo check`/`cargo test` pasa.
- [ ] El comportamiento coincide con `docs/architecture/sfx.md`.
- [ ] `CueMessage { id: CueId, kind: CueKind }` implementado y registrado en Bevy usando `.add_message::<CueMessage>()`.
- [ ] El sistema `log_audio_cue` filtra por `kind: Audio` y loguea a `debug!` el cue.
- [ ] Sistema de demostración de modulación continua en `Update` que lee `BodyVelocity`/`Stamina` de entidades con `Actor` y simula (loguea con debounce o rastrea cambios) modulación de pitch/volumen para verificar que la separación simulación/presentación (Constitución §20) funciona sin allocations en hot path.
