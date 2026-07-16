# Projectiles

**Carpeta objetivo:** `src/projectiles/`

**Estado:** implementado (ticket `combat-bow`, 2026-07-15) para flechas del
arco; daño real espera `health-core` (hoy: cue de log).

Cuerpos físicos simples (flechas) con un único comportamiento — volar hasta
impactar — no actores multi-estado. No usan el patrón Broker de
Movement/Combat/Mounts. Ver `rationale/when-not-broker-pattern.md`.

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `Arrow` | `projectiles/mod.rs` | `{ velocity, shooter, damage, remaining, stuck }`. Dueño exclusivo de su propio vuelo. (El nombre genérico `Projectile` llegará si aparece un segundo tipo.) |
| `SpawnProjectileMessage` | `projectiles/mod.rs` | `{ shooter, origin, velocity, damage }` (velocity ya compuesta — el emisor decide dirección×rapidez). Combate lo emite al soltar la flecha; Projectiles construye la entidad — Combate nunca hace `commands.spawn` de una flecha. Consumido al tick siguiente (~16 ms, latencia aceptada, mismo criterio que constraints). |

## Sistemas (comportamiento) — implementado

Todo en `FixedUpdate` (simulación, determinístico para replicación):

1. **Spawn** — `MessageReader<SpawnProjectileMessage>` crea la entidad `Projectile`. Para prevenir problemas de "tunneling" (flechas que atraviesan muros delgados debido a su alta velocidad), la entidad se configura sin un `RigidBody::Dynamic` ordinario, o bien se le activa la detección continua de colisiones (CCD) de Avian.
2. **Integrate** — En `FixedUpdate`, se realiza una integración de posición (vuelo en línea recta o parábola balística simple).
3. **CollideAndDamage** — barrido `cast_ray` a lo largo del paso del tick (sin tunneling), excluyendo al tirador. Impacto contra colliders en `GameLayer::Actor`: daño con bonus ×4 contra objetivo no alertado (lee `enemies::Awareness`; la flecha no exige Sneak — contrato en `combat.md`), emite `combat::HitImpactMessage` (feedback), `enemies::DirectThreatMessage` (aggro con posición aproximada del tirador) y `movement::BodyImpulseMessage` (knockback ligero); `health::DamageRequestMessage` cuando exista Health (hoy: cue de log). Contra mundo: la flecha se clava y se desvanece (decisión tomada: persiste 4 s). TTL de vuelo 8 s.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Combate emite `SpawnProjectileMessage` al soltar el arco (`CombatState::Active` tras `Aiming`) | MESSAGE | Combate no conoce la forma interna de `Projectile` |
| Projectiles emite `health::DamageRequestMessage` al impactar | MESSAGE | Ver `docs/architecture/health.md` |
| Multiplayer: solo el Host simula vuelo/colisión; clientes reciben transform replicado | SHARED-CONTRACT | Mismo set `AuthoritativeSimulation` que Movement/Combat/Mounts — ver `rationale/multiplayer-gating.md` |

## Decisiones abiertas

- ~~Flecha clavada vs. despawn~~ — decidido: se clava 4 s y desaparece.
- ~~Gravedad vs. línea recta~~ — decidido: parábola (gravedad compartida de
  Movement).
- Otros proyectiles futuros (lanza arrojadiza, bomba) — mismo sistema o uno
  nuevo, evaluar cuando existan.
