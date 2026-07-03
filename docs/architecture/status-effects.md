# Status Effects

**Carpeta objetivo:** `src/status_effects/`

Frío/calor, mojado y "tormenta atrae metal" (GDD §10). Cruza
World↔Movement↔Combate↔Inventory — no vive dentro de ninguno de esos
sistemas porque ninguno es dueño natural del dato (ej. Movement no debería
saber de daño por frío, Health no debería saber de clima).

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `StatusEffects` | `status_effects/mod.rs` | Componente por actor: array de capacidad fija `[Option<StatusEffect>; N]` (sin `Vec`, Constitución §18 — esto sí corre cada `FixedUpdate`). |
| `StatusEffect` | `status_effects/effect.rs` | Dato puro: `{ kind: StatusKind, intensity: f32 }`. `StatusKind`: `Cold`, `Heat`, `Wet`, `LightningAttraction`. |
| `WetExposureMessage` | `status_effects/messages.rs` | Pedido semántico `{ actor, intensity }` emitido por Swim/Dive al salir del agua. StatusEffects es el único sistema que escribe el componente `StatusEffects`. |

## Sistemas (comportamiento) — propuesta

Pipeline propio en `FixedUpdate`, sin Broker (no hay "un estado activo" que
arbitrar — varios efectos coexisten a la vez, ej. mojado *y* frío juntos):

1. **EvaluateExposure** — lee `World::Weather`/`TimeOfDay` y la posición del
   actor (bioma) y escribe/actualiza `StatusEffects` (sube `Cold` con nieve,
   sube `Wet` con lluvia o al salir de `swim.md`, etc.).
2. **ApplyConsequences** — por cada efecto activo con intensidad sobre
   umbral, emite `health::DamageRequestMessage` (frío/calor extremos) — Health no
   sabe de clima, solo aplica el monto (`rationale/health-ownership-boundary.md`).
3. **ApplyWetExposure** — consume `WetExposureMessage` y actualiza únicamente
   el efecto `Wet` del actor afectado.
4. **LightningStrike** — durante `Weather::Storm`, si el actor tiene equipo con tag de material metálico (lee `inventory::EquipmentSlots`), probabilidad de impacto → emite `health::DamageRequestMessage` de tipo eléctrico al actor y emite `CueMessage` para que VFX/SFX reproduzcan el rayo y el trueno en su posición.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| StatusEffects lee `World::Weather`/`TimeOfDay` | READ | Query read-only sobre los `Resource` de `world.md` |
| StatusEffects lee `inventory::EquipmentSlots` (tags de material) | READ | Para la mecánica de rayo atrae metal |
| StatusEffects emite `health::DamageRequestMessage` (frío/calor/eléctrico) | MESSAGE | Ver `health.md` — `DamageKind` necesita variantes para estos casos (ya listado en Decisiones abiertas de `health.md`) |
| Movement lee `StatusEffects::Wet` para reducir agarre al escalar (GDD §10) | READ | `motors::climb`/`services::ground` leerían la intensidad de `Wet`, sin decidir todavía si es un multiplicador directo o vía facts |
| Swim/Dive pide aplicar `Wet` al salir del agua | MESSAGE | Swim/Dive emite `WetExposureMessage`; StatusEffects valida y muta su propio componente |

## Decisiones abiertas

- Umbrales concretos (cuánta intensidad de `Cold` antes de dañar, GDD deja
  esto abierto).
- Cómo se contrarresta (equipo, elixires de Crafting) — depende de
  `inventory.md`/`crafting.md`.
- Si `Wet` decae con el tiempo o requiere secarse activamente (fogata).
- Definición de biomas que alimentan `EvaluateExposure` (GDD, abierto en
  `world.md`).
