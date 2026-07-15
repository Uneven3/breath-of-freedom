# World

**Carpeta objetivo:** `src/world/`

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `TimeOfDay` | `world/clock.rs` (Resource) | Ciclo día/noche normalizado (0.0–1.0). Solo el sistema de reloj lo avanza. |
| `Weather` | `world/weather.rs` (Resource) | `{ kind: WeatherKind, intensity: f32 }`. `WeatherKind`: `Clear`, `Rain`, `Storm`, `Snow`. Solo el sistema de clima lo escribe. |

Terreno/colliders estáticos no son datos de simulación por actor — son
geometría del mundo, cubierta por Avian.

`world::GameLayer` (ya implementado en `src/world.rs`) es el vocabulario de
capas de física de todo el juego: la geometría estática queda en `Default`
(capa 0, sin componente) y los actores cinemáticos declaran `Actor`. Las
capas no alteran contactos físicos; permiten que las queries espaciales
(p. ej. el ledge sensing de Movement) elijan qué ven vía
`SpatialQueryFilter::from_mask`.

## Sistemas (comportamiento) — propuesta

- `advance_clock` — avanza `TimeOfDay` en `FixedUpdate` (determinístico,
  igual que Movement). En clientes de multiplayer, este sistema se cancela
  para respetar los datos replicados del host.
- `advance_weather` — transición de `Weather` según reglas climáticas del
  mundo. En clientes de multiplayer, este sistema se cancela igual que el
  reloj para respetar los datos replicados del host.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Movement lee `Weather` (agarre al escalar en lluvia, GDD §10) | READ | Query read-only sobre el `Resource` |
| Enemies lee `TimeOfDay` (spawn/comportamiento día-noche, GDD §10) | READ | Query read-only sobre el `Resource` |
| SFX/VFX derivan cues ambientales de `Weather`/`TimeOfDay` | READ → MESSAGE | Ver `rationale/presentation-cues.md` |
| Multiplayer: `TimeOfDay`/`Weather` son estado compartido de la sesión | SHARED-CONTRACT | Solo el host los simula y los replica (ver `rationale/multiplayer-model.md`) |
| StatusEffects, Swim/Dive, Snowboard, NPCs/Quests leen `Weather`/`TimeOfDay` | READ | Mismo `Resource` read-only — ver `status-effects.md`, `swim.md`, `snowboard.md`, `npcs.md` |

World no lee ni escribe nada de otros sistemas — es sustrato.

## Decisiones abiertas

- Reglas concretas de clima (probabilidad de tormenta, cuánto penaliza el
  agarre la lluvia, mecánica de "tormenta atrae metal").
- Definición de biomas.
- Tamaño del mundo y modelo de persistencia (GDD §13).
