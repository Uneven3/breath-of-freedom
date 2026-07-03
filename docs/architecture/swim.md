# Nadar / Bucear

**Carpeta objetivo:** `src/movement/` (motores nuevos, mismo plugin de
Movement — ver `rationale/traversal-extensions-in-movement.md` para por qué
esto no es un plugin nuevo).

Exploración acuática con profundidad, buceo, aliento/oxígeno, corrientes y
visibilidad reducida bajo el agua (GDD §9, línea Fontaine/Genshin) — no solo
cruzar de un punto a otro.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `LocomotionState::Swim`, `::Dive` | `movement/state.rs` | Dos variantes nuevas del mismo enum SSoT — no un enum propio. |
| `WaterFacts` | `movement/facts.rs` | Hecho calculado por un servicio nuevo (`services/water.rs`): `{ submerged: bool, depth: f32, current: Vec3, surface_y: f32 }`. Mismo patrón que `GroundFacts`/`LedgeFacts` — lo calcula un servicio, lo leen los motores. |
| `Oxygen` | `movement/oxygen.rs` | Pool propio, no reutiliza `Stamina` — nadar en superficie no debería drenar aire, solo `Dive` lo hace. Mismo patrón de mutación encapsulada (`drain`/`recover`) que `Stamina`. |

## Estados nuevos

- **`Swim`** — en superficie, movimiento 2D horizontal, gateado por
  `Stamina` (igual que Sprint) para corrientes fuertes, no por `Oxygen`.
- **`Dive`** — bajo la superficie, movimiento 3D, gatea por `Oxygen`;
  `Oxygen` en 0 dispara daño (ver Relaciones).

## Sistemas (comportamiento) — propuesta

- `services::water` — nuevo servicio en `MovementSet::SenseWorld`, escribe
  `WaterFacts` a partir de volúmenes de agua del mundo (Avian sensor/trigger
  o SDF de World, sin decidir todavía).
- `motors::swim::propose` / `motors::dive::propose` — nuevos motores en
  `MovementSet::GatherProposals`, siguen el mismo contrato `propose()` que
  los 13 motores existentes.
- Simulación de `Oxygen` (`update_oxygen`) corre como un sistema global en `FixedUpdate` para cualquier entidad con `Oxygen`: drena aire si el estado activo es `Dive`, y recupera aire en cualquier otro caso. Esto evita tener que inyectar dependencias y lógica de recuperación de oxígeno en todos los motores terrestres no relacionados.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| `Oxygen` llega a 0 en `Dive` → daño de ahogo | MESSAGE | Emite `health::DamageRequestMessage` (`health.md`), Movement no decide consecuencias de salud, solo notifica |
| World: corrientes/profundidad provienen de geometría/volúmenes del mundo | READ | `services::water` lee volúmenes de agua definidos por World |
| Combate: `LocomotionState::Swim`/`Dive` restringe qué acciones de combate son válidas (¿se puede apuntar el arco nadando?) | decisión abierta | Análogo a cómo Combate ya lee `Sneak`; reglas exactas abiertas |
| StatusEffects: salir del agua deja al actor "Mojado" (GDD §10) | MESSAGE | Emite `status_effects::WetExposureMessage`; StatusEffects es el único dueño que escribe `StatusEffects::Wet` |

## Decisiones abiertas

- ¿`Dive` tiene su propio límite de profundidad (colapso de presión) o solo
  `Oxygen` lo gatea?
- Visibilidad reducida bajo el agua: ¿es un parámetro de Camera/VFX (niebla)
  leyendo `WaterFacts`, o pertenece a World?
- Cómo interactúan corrientes con `BodyVelocity` (¿fuerza aplicada, o
  desplazamiento directo?).
- Transición `Fall → Dive` al entrar al agua desde una caída.
