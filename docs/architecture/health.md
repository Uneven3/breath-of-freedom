# Health

**Carpeta:** `src/health/`
**Estado:** implementado; checkpoint jugado pendiente.

Health posee el pool de HP y su mutación autoritativa. Las reglas que calculan
daño viven en Combat, Projectiles, Mounts o hazards y llegan por mensaje.

## Contratos

| Tipo | Función |
|---|---|
| `Health` | Pool `{ current, max }` privado; solo `apply_damage`/`heal_full` mutan. |
| `DamageRequestMessage { target, amount, source }` | Pedido atribuido; `source: None` representa hazards sin autor. |
| `HostileInteractionImmunity(Entity)` | Política genérica de interacción completa por fuente. |
| `DeathMessage { entity }` | Cruce a cero, exactamente una vez. |

No existe `DamageAppliedMessage`. Se agregará únicamente cuando haya un
consumidor real; Parry/Staggered tampoco están implementados todavía.

## Política de interacción hostil

La inmunidad del horse frente a su owner no es solo de HP. Todo productor de
efectos hostiles debe consultar `HostileInteractionImmunity` antes de emitir
daño, knockback, threat o feedback de impacto. Melee y Projectiles lo hacen;
Mounts Charge aplica la misma regla a su fuente. `apply_damage` repite la
validación como última línea autoritativa para pedidos que lleguen por otra
ruta. `source: None` nunca coincide y los hazards siguen dañando.

## Ordering y muerte

`HealthSet::Apply` corre después de `ProjectilesSet::Simulate` y
`MountsSet::Charge`, por lo que aplica todo el daño del tick. Player respawnea,
Enemies y targets de práctica despawnean, y Mounts marca el horse pendiente,
libera al rider en el lifecycle siguiente y solo entonces lo despawnea.

## Abierto

- `DamageKind`, resistencias y daño ambiental.
- Regeneración/comida.
- Un resultado aplicado/rechazado si aparece un consumidor concreto.
