# Mounts

**Carpeta:** `src/mounts/`
**Estado:** refactor Actor/lifecycle implementado; checkpoint jugado final pendiente.

El horse es un `Actor` normal de Movement. Mounts posee únicamente la relación
jinete–montura, owner persistente, reglas de montar/desmontar, carga y requests
de debug. Movement sigue siendo el único escritor de `Transform`,
`BodyVelocity`, collider e `Intents` de actores.

## Composición y datos

El horse compone `KinematicActorBundle`, Ground/Sprint/Stairs/Airborne/Jump,
`Stamina`, `Health(120)` y `HorseCharge`. Sus perfiles alcanzan 9 m/s a paso,
13 m/s en sprint y 6.5 m/s de impulso de salto. No recibe Sneak, crouch,
Climb, Glide, Ladder, LedgeTraversal, Mantle ni WallJump.

| Tipo | Dueño | Función |
|---|---|---|
| `Horse` | Mounts | Marker de dominio sobre un `Actor`. |
| `MountedOn(Entity)` / `RiddenBy(Option<Entity>)` | Mounts | Relación uno-a-uno mantenida en ambos extremos. |
| `HorseOwner(Option<Entity>)` | Mounts | Owner persistente; cambiar de rider no lo reemplaza. |
| `HorseCharge` | Mounts | Fase, generación y posición anterior del sweep. |
| `KinematicAttachment` / `LocomotionEnabled` | Movement | Silla y suspensión/reactivación del pipeline físico. |
| `ControlRedirect` | Movement | Redirect persistente con máscara planar/sprint/jump. |
| `HostileInteractionImmunity(owner)` | Health | Bloquea HP y efectos hostiles atribuidos al owner. |
| `CombatContext` / `MountedCombatProfile` | Combat | Selección read-only y tuning efectivo de espada/arco. |

## Lifecycle y ordering

E usa un `MountInputCursor` propio. El candidato debe estar libre y a no más
de 2.5 m. La transición emite un request atómico a Movement y solo escribe
`MountedOn`/`RiddenBy` después del ack aceptado; un rechazo deja ambos extremos
sin cambios. El desmontaje voluntario prueba candidatos fijos con suelo,
pendiente, cápsula y headroom válidos. Sin candidato se rechaza. F8 y muerte
usan búsqueda radial y, si toda validación falla, solicitan un detach de
emergencia. Ese detach libera la relación y permite despawnear el horse, pero
no reactiva colisión ni locomoción en la pose no validada; hereda solo la
velocidad planar.

```text
Mounts Request/Lifecycle
  -> Movement ApplyExternal
  -> Mounts Confirm
  -> Movement Read/Redirect/Sense/Gather/Arbitrate/Tick/Sync
  -> Mounts PostMove
  -> Combat ApplyContext/Read/Gather/Arbitrate/Tick
  -> Projectiles Simulate
  -> Mounts Charge
  -> Health Apply
  -> Mounts DeathCleanup
```

`MovementSet::ApplyExternal` emite el ack; `MountsSet::Confirm` actualiza la
relación y publica el contexto, y `MountsSet::PostMove` recién entonces puede
despawnear un horse liberado. Un rechazo `CapacityPending` reencola el request
exacto y se reintenta después de la preparación de capacidad en `Update`.

La muerte emite el desmontaje al lifecycle siguiente: el horse permanece
`PendingHorseDespawn` y nunca desaparece antes de liberar al rider.
F8 solo captura una solicitud en `Update`; spawn/despawn se decide en
`FixedUpdate`. Un detach de emergencia o carrier desaparecido instala
`PendingSafeRecovery`: Movement prueba cuatro alturas deterministas por tick,
sin colecciones temporales, y conserva `ColliderDisabled` y locomoción
suspendida hasta encontrar una pose sin overlap. Si desaparece el rider,
Mounts limpia `RiddenBy` y pide a Movement neutralizar el horse.

## Charge

Se activa al sprintar a `>= 11 m/s`, permanece hasta `<= 10 m/s` (histéresis)
y barre una esfera entre la posición anterior y la actual. `SpatialQuery`
selecciona solo `Enemy`; un raycast que ignora actores descarta candidatos
ocultos por geometría. Cada `(horse, generation, enemy)` emite una sola vez
`DamageRequestMessage`, `BodyImpulseMessage` y `HitImpactMessage`.

El ledger no tiene límite fijo: `Update` reserva capacidad según horses ×
enemies vivos; `FixedUpdate` no hace crecer el heap. Si entidades aparecen
después de la preparación, el hit se difiere, la posición previa no avanza y
el mismo tramo se reintenta después de que `Update` amplíe capacidad.

Los tests con el backend espacial real de Avian cubren tunneling entre
endpoints a alta velocidad, pared oclusora y separación vertical, además de
dedup/rearm y más de 16 objetivos con dos horses.

## Presentación

`visuals.rs` crea un `HorseVisual` separado enlazado por `VisualOf`; la entidad
de simulación no contiene meshes, materiales ni assets. El cleanup tolera que
el horse ya haya desaparecido. `assets/Prototype.glb` se conserva sin uso ni
atribución: incorporarlo o retirarlo requiere una decisión humana separada.

## Abierto

- Monturas voladoras y soporte para enemigos montados.
- Vínculo/doma y persistencia del owner.
- Checkpoint jugado de feeling y escenarios físicos de carga/desmontaje.
