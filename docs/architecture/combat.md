# Combat

**Carpeta:** `src/combat/`
**Estado:** melee, arco, perfiles montados y consecuencias implementados;
checkpoint de feeling pendiente. Guard, Parry y Staggered no están
implementados.

Combat es un pipeline Broker hermano de Movement. Opera sobre `Actor`, posee
`CombatIntents`, `CombatState`, propuestas y estado privado por actor; nunca
escribe `Transform`, `BodyVelocity`, `LocomotionState` ni HP.

## Datos y estados

| Tipo | Función |
|---|---|
| `CombatIntents` | Snapshot semántico de Attack/Aim escrito por un Brain local o IA. |
| `CombatState` | SSoT: `Idle`, `Windup`, `Active`, `Recovery`, `Aiming`. |
| `WeaponProfile` / `AttackStep` | Arma base equipada y combo de capacidad fija. |
| `ComboLocal` | Paso, reloj, buffer y snapshot del perfil efectivo al iniciar acción. |
| `ActiveSwing` | Dedup por objetivo durante el swing actual. |
| `DrawStrength` | Carga/cooldown y snapshot de `BowProfile` al iniciar draw. |
| `CombatContext` | Solo selección mounted; no guarda ni reemplaza el arma base. |
| `MountedCombatProfile` | Tuning alternativo Combat-owned para espada y arco. |

## Contexto montado y snapshots

Mounts emite `SetMountedCombatMessage`; `CombatSet::ApplyContext` lo consume
antes de `ReadIntents`. Combat resuelve read-only el perfil efectivo:
`WeaponProfile` base a pie o `MountedCombatProfile::sword` montado. Al entrar
al primer `Windup`, `ComboLocal` copia el perfil efectivo y el combo completo
usa esa copia. Equipar/montar/desmontar durante una acción no cambia alcance,
arco, daño ni timings ya iniciados.

El arco aplica la misma regla: al primer tick de carga, `DrawStrength`
snapshottea speed/damage/draw-time. Soltar interpola dentro de ese tuning y
emite `SpawnProjectileMessage`. Un cambio de contexto durante el draw no lo
retunea. El perfil mounted actual sacrifica velocidad máxima por draw más
rápido y tuning propio de daño; el de espada usa mayor alcance/arco.

## Pipeline y ordering

```text
ApplyContext -> ReadIntents -> GatherProposals -> Arbitrate
             -> TickActiveMotor -> EmitConstraints
```

Corre después de `MovementSet::SyncAttachments`, por lo que cámara/hitboxes
leen la pose de silla del mismo tick. Projectiles corre después de
`EmitConstraints`; Health aplica después de Projectiles y Charge.

El motor melee propone `Windup -> Active -> Recovery`; ataques bufferizados
encadenan dentro de `chain_window_secs`. Durante Active, un sphere sweep
enmascarado a `GameLayer::Actor` y filtrado por arco emite `MeleeHitMessage`
una vez por objetivo. La resolución calcula sigilo, luego emite:

- `DamageRequestMessage` con `source: Some(attacker)`;
- `BodyImpulseMessage` para knockback;
- `DirectThreatMessage` para aggro;
- `HitImpactMessage` para presentación.

Antes de emitir cualquiera de ellos consulta
`HostileInteractionImmunity`; una fuente bloqueada no produce HP, feedback,
threat ni knockback. Projectiles aplica la misma política.

Estados comprometidos emiten `LocomotionConstraintMessage::ForbidSprint`.
Movement lo consume al tick siguiente y sigue siendo quien decide la
locomoción final.

## Presentación

`visuals.rs` lee el `AttackStep` snapshotteado para el arco de swing;
`presentation/juice.rs` y SFX consumen mensajes tolerando que el actor haya
despawneado. La simulación no lee cámara, mesh, materiales ni hitstop.

## Abierto

- Guard/Parry/Staggered y su contrato de resultado de daño.
- Equipment/Inventory/durabilidad y cambio real de `WeaponProfile` base.
- Lock-on y checkpoint jugado de timings/tuning.
