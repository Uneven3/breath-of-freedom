# SFX

**Carpeta objetivo:** `src/sfx/`

Consumidor del bus de cues compartido para disparos discretos, y lector
read-only de simulación para parámetros continuos — ver
`rationale/presentation-cues.md`.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `CueMessage` | `presentation/cues.rs` (compartido con VFX, no propio de SFX) | `{ id: CueId, kind: CueKind }`, derivado con `#[derive(Message)]`. SFX filtra por `kind: Audio`. |

SFX no define su propio tipo de mensaje — sería una segunda fuente de verdad
para lo mismo que ya resuelve `CueMessage`.

## Sistemas (comportamiento) — propuesta

- `log_audio_cue` — `MessageReader<CueMessage>` filtrado a `Audio`, hace
  `log::debug!("[audio] cue: {id}")` (GDD §6: placeholder obligatorio hasta
  que exista audio real). Corre en `Update`.
- Reproducción real de audio: reemplaza el `log::debug!` cuando haya assets
  — mismo punto de entrada, sin tocar quién emite `CueMessage`.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| SFX lee `CueMessage` emitido por simulación o por una cola de transición consumida en `Update` | MESSAGE | Disparos discretos: pasos, impactos, activaciones; no depende solo de `Changed<T>` |
| SFX lee estado de simulación (`BodyVelocity`, `Stamina`, `Weather`) | READ | Modulación continua de loops: viento, respiración, lluvia |

SFX no escribe simulación. Puede leer componentes/resources de simulación en
`Update` para modular parámetros continuos; `CueMessage` no debe usarse para
valores que cambian cada frame.

## Decisiones abiertas

- Motor de audio real y sus assets (GDD §6).
- Mezcla/volumen por categoría (ambiente vs. combate).
