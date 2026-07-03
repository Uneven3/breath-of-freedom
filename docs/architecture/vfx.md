# VFX

**Carpeta objetivo:** `src/vfx/`

Consumidor del mismo bus de cues que SFX para disparos discretos, y lector
read-only de simulación para parámetros continuos — ver
`rationale/presentation-cues.md`.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `CueMessage` | `presentation/cues.rs` (compartido con SFX) | Deriva `#[derive(Message)]`; VFX filtra por `kind: Vfx`. |

## Sistemas (comportamiento) — propuesta

- `spawn_vfx_cue` — `MessageReader<CueMessage>` filtrado a `Vfx`; puede usar
  placeholder de debug hasta que exista el sistema de partículas real.
- Spawneo real de partículas: reemplaza el placeholder cuando haya assets.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| VFX lee `CueMessage` emitido por simulación o por una cola de transición consumida en `Update` | MESSAGE | Disparos discretos: polvo al aterrizar, chispa al parry, activación de planeo; no depende solo de `Changed<T>` |
| VFX lee estado de simulación (`Transform`, `BodyVelocity`, `Weather`) | READ | Modulación continua de emisores: estelas, intensidad de lluvia, turbulencia |

VFX no escribe simulación. Puede leer componentes/resources de simulación en
`Update` para modular parámetros continuos; `CueMessage` queda reservado para
sucesos discretos.

## Decisiones abiertas

- Sistema de partículas real (¿`bevy_hanabi`? requiere aprobación, §17).
- Catálogo de cues visuales (polvo al aterrizar, chispa al parry, estela de
  planeo).
