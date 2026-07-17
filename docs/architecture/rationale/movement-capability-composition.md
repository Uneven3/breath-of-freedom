# Rationale: composición granular de capacidades de Movement

**Estado:** revisión aprobada para planificación; implementación pendiente.

## Problema

El pipeline multi-actor de Movement es la base correcta para Player, enemigos,
animales, monturas y vehículos cinemáticos. Reutilizarlo evita duplicar
servicios de suelo y escaleras, gravedad, colisiones, arbitraje e impulsos.

El componente actual `GroundMovement`, sin embargo, concede y configura Walk,
Sprint, Sneak y Stairs como un bloque. `GroundMovementBundle` agrega además
estado privado de sprint, crouch, sneak y stairs a cualquier actor terrestre.
Por eso el horse puede estructuralmente hacer Sneak aunque su redirect descarte
ese input. El filtro de input oculta una composición inválida; no la resuelve.

También se mezclan dos preguntas diferentes:

1. Qué acciones puede ejecutar un actor.
2. Cómo acelera, frena, gira y conserva inercia al ejecutarlas.

Cambiar solo `max_speed`, `acceleration` y `friction` produce actores con
velocidades diferentes, pero con la misma dinámica básica. Un horse termina
sintiéndose como un humano rápido en vez de un cuerpo con steering, inercia y
versatilidad propios.

## Decisión

Movement conserva un único pipeline:

```text
controller -> Intents -> facts -> proposals -> arbitration -> active motor
```

La reutilización se divide en tres capas independientes:

| Capa | Representación | Pregunta |
|---|---|---|
| Capacidad persistente | Presencia de un `Component` | ¿Puede hacerlo? |
| Perfil persistente | Datos dentro del componente | ¿Cómo lo hace? |
| Estado runtime | `LocomotionState` + estado local del motor | ¿Qué hace ahora? |

No se agregan ramas `if Horse`, `if Player` o `if Enemy` en Movement. Una
especie nueva se construye componiendo capacidades y perfiles. Una regla
física cualitativamente nueva se agrega mediante un motor/capacidad nuevos que
reutilizan servicios y helpers existentes.

## Capacidades terrestres objetivo

`GroundMovement` deja de ser una macro-capacidad. Se separa en:

| Capacidad | Responsabilidad |
|---|---|
| `GroundMovement` | Movimiento terrestre base y perfil de drive normal. |
| `SprintMovement` | Marcha rápida, perfil de drive y reglas de stamina propias. |
| `SneakMovement` | Postura agachada, collider, clearance, lock y perfil de drive propio. |
| `StairsMovement` | Permiso y tuning para recorrer escaleras authored. |
| `JumpMovement` | Salto básico; permanece independiente. |
| `AirborneMovement` | Caída y control aéreo base; permanece independiente. |

Cada motor propone solo para actores con su capacidad. Un actor sin
`SneakMovement` no puede proponer `LocomotionState::Sneak` aunque sus `Intents`
contengan una solicitud inválida.

Los bundles siguen siendo ergonomía de construcción, nunca capacidades:

- `GroundMovementBundle`: `GroundMovement` y solo estado del drive base.
- `SprintMovementBundle`: `SprintMovement` + `SprintLock`.
- `SneakMovementBundle`: `SneakMovement` + crouch, collider y locks.
- `StairsMovementBundle`: `StairsMovement` + `StairsFacts`/`StairsLocal`.
- Los bundles existentes de Jump, Glide y traversal conservan la misma regla.

## Perfil de drive terrestre

Walk, Sprint y Sneak comparten un kernel físico, no un preset universal. El
perfil debe expresar al menos las reglas que un sistema realmente consuma:

```rust
pub struct GroundDriveProfile {
    pub max_forward_speed: f32,
    pub max_reverse_speed: f32,
    pub forward_acceleration: f32,
    pub reverse_acceleration: f32,
    pub coast_deceleration: f32,
    pub brake_deceleration: f32,
    pub velocity_alignment_rate: f32,
    pub turn_rate_at_zero_speed: f32,
    pub turn_rate_at_max_speed: f32,
    pub turning_speed_loss: f32,
}
```

Los nombres y campos definitivos aterrizan junto con el sistema que los lee;
no se guardan datos especulativos. El modelo sí debe distinguir:

- Acelerar hacia adelante y en reversa.
- Soltar input frente a frenar o invertir dirección.
- Reorientar la velocidad frente a rotar el cuerpo visual/físico.
- Capacidad de giro en reposo frente a máxima velocidad.
- Pérdida de velocidad en curvas cerradas.

Los motores Walk/Sprint/Sneak seleccionan su perfil y llaman un helper común
sin allocations, por ejemplo `ground_drive_step`. La lógica compartida
resuelve aceleración, braking, steering, rotación y `move_and_slide`.

Un perfil Player preserva la respuesta validada: aceleración lateral alta,
frenado rápido y giro casi inmediato. Un perfil Horse usa respuesta lateral
baja, arranque progresivo, frenado largo y giro limitado por velocidad. Un
enemigo pesado puede reutilizar el mismo kernel con otro preset.

## Cuándo agregar otro motor

No se fuerza toda locomoción terrestre dentro de un perfil omnipotente. Si el
checkpoint demuestra que el horse requiere una regla cualitativamente distinta
—por ejemplo movimiento no holonómico, radio mínimo de giro o pasos
comprometidos— se agrega una capacidad/motor como `SteeredGroundMovement`.

Ese motor sigue reutilizando:

- `GroundFacts` y `StairsFacts`.
- Ground/Stairs sensing.
- `ProposalBuffer` y arbitraje.
- `body_move_and_slide` e impulsos.
- El schedule normal de Movement.

Crear un motor específico no autoriza duplicar el pipeline ni consultar el
marker `Horse` desde Movement.

## Composición objetivo

| Actor | Ground | Sprint | Sneak | Stairs | Jump | Airborne |
|---|---:|---:|---:|---:|---:|---:|
| Player | sí | sí | sí | sí | sí | sí |
| Horse | sí | sí | no | sí | sí | sí |
| Bokobo melee | sí | según perfil | no | sí | no | sí |
| Animal pequeño | sí | sí | no | según especie | según especie | sí |

El horse comparte Stairs porque la regla general —detectar el tramo, seguir su
tangente y mantener contacto— es reusable. Su `StairsMovement` configura
pendiente/peldaño permitido, velocidad, steering y restricciones propias.

## Núcleo mínimo del actor

`KinematicActorBundle` debe converger hacia el contrato realmente obligatorio:
cuerpo cinemático, transform, collider/capas, dimensiones, velocity, intents,
estado/propuestas, contacto, ground sensing/facts, LOD y constraints.

Datos opcionales como Stamina, `LedgeFacts`, `StairsFacts`, `LadderFacts` y
estado privado de motores pertenecen a bundles de capacidad/pool. La migración
se hace en tickets separados para no romper queries de todos los motores a la
vez.

## Intents

`Intents` puede seguir siendo un snapshot semántico amplio durante esta
migración. La presencia de capacidad es la autorización definitiva. A medio
plazo, `GaitIntent` debe separar intensidad (`wants_sprint`) de postura
(`wants_sneak`), porque sprint y sneak no son variantes equivalentes de un
mismo concepto anatómico.

## Invariantes

- Un actor sin capacidad nunca propone ni ejecuta su estado.
- Movement no consulta markers de dominio como `Horse`, `Player` o `Enemy`.
- Dos actores con igual input pueden acelerar/frenar/girar distinto por datos.
- Cada estado runtime conserva un único motor dueño.
- Los bundles no activan sistemas y contienen solo dependencias de su capacidad.
- Agregar una especie común es composición; agregar una física nueva es una
  extensión aditiva.
- No hay allocations en `FixedUpdate`.

## Validación obligatoria

- Un horse con intents Sneak nunca propone Sneak y carece de todos sus datos.
- Player y Horse reciben igual planar input y producen curvas de aceleración,
  frenado y giro diferentes.
- Los valores Player preservan el checkpoint previo.
- Stairs selecciona Player y Horse solo si tienen `StairsMovement`.
- Tests de composición verifican ausencia, no solo presencia, de capacidades.
- Un grep/test arquitectónico impide dependencias de Movement hacia markers de
  especies.
