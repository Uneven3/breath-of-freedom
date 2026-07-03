# Projectiles

**Carpeta objetivo:** `src/projectiles/`

Cuerpos físicos simples (flechas) con un único comportamiento — volar hasta
impactar — no actores multi-estado. No usan el patrón Broker de
Movement/Combat/Mounts. Ver `rationale/when-not-broker-pattern.md`.

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `Projectile` | `projectiles/mod.rs` | Marker + `{ velocity: Vec3, damage: f32, source: Entity }`. Dueño exclusivo de su propio vuelo. |
| `SpawnProjectileMessage` | `projectiles/messages.rs` | `{ origin, direction, speed, damage, source }`. Combate lo emite al soltar la flecha; Projectiles decide cómo construir la entidad — Combate nunca hace `commands.spawn` de una flecha directamente. |

## Sistemas (comportamiento) — propuesta

Todo en `FixedUpdate` (simulación, determinístico para replicación):

1. **Spawn** — `MessageReader<SpawnProjectileMessage>` crea la entidad `Projectile`. Para prevenir problemas de "tunneling" (flechas que atraviesan muros delgados debido a su alta velocidad), la entidad se configura sin un `RigidBody::Dynamic` ordinario, o bien se le activa la detección continua de colisiones (CCD) de Avian.
2. **Integrate** — En `FixedUpdate`, se realiza una integración de posición (vuelo en línea recta o parábola balística simple).
3. **CollideAndDamage** — Para máxima precisión a alta velocidad, se prefiere un barrido espacial (`SpatialQuery::cast_ray` o `cast_shape`) a lo largo del vector de movimiento de ese frame. Si se detecta un impacto, se calcula la posición exacta del choque, se emite `health::DamageRequestMessage` al `target` (si corresponde), y se destruye el proyectil. Si transcurren N segundos sin colisión, se despawnea por tiempo límite.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Combate emite `SpawnProjectileMessage` al soltar el arco (`CombatState::Active` tras `Aiming`) | MESSAGE | Combate no conoce la forma interna de `Projectile` |
| Projectiles emite `health::DamageRequestMessage` al impactar | MESSAGE | Ver `docs/architecture/health.md` |
| Multiplayer: solo el Host simula vuelo/colisión; clientes reciben transform replicado | SHARED-CONTRACT | Mismo set `AuthoritativeSimulation` que Movement/Combat/Mounts — ver `rationale/multiplayer-gating.md` |

## Decisiones abiertas

- Flecha clavada (persiste visualmente tras impactar terreno) vs. despawn
  inmediato — es una decisión de presentación, no bloquea la arquitectura.
- Gravedad/arco de vuelo de la flecha vs. línea recta — mecánica de combate,
  no arquitectura.
- Otros proyectiles futuros (lanza arrojadiza, bomba) — mismo sistema o uno
  nuevo, evaluar cuando existan.
