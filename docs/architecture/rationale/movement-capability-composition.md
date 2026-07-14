# Rationale: capacidades persistentes de Movement

## Objetivo

La locomocion debe poder componerse por actor sin duplicar motores ni bifurcar
el pipeline. Link, una criatura humanoide y una montura pueden compartir un
motor con perfiles distintos; una montura puede omitir por completo las
capacidades que no le corresponden.

El objetivo no es convertir cada estado de `LocomotionState` en un componente.
El estado activo sigue siendo exclusivo y representa lo que el actor esta
haciendo ahora. Las capacidades persistentes representan las acciones que el
actor puede llegar a ejecutar y sus parametros por actor.

## Modelo

Un actor conserva el pipeline existente:

```text
controlador -> Intents -> facts -> propuestas -> arbitraje -> motor activo
```

- Un controlador de jugador, IA o red escribe el mismo `Intents`.
- Los servicios publican hechos fisicos; no conceden capacidades.
- Cada motor se ejecuta en el schedule normal y selecciona actores mediante
  `Query` con el componente de capacidad que necesita.
- `LocomotionState` sigue siendo la unica fuente de verdad del modo activo.
- Solo el motor del estado activo escribe el movimiento del actor durante el
  tick.

Por tanto, agregar `ClimbMovement` no activa ni desactiva sistemas: permite
que los sistemas de Climb seleccionen a esa entidad y usen sus valores.

## Taxonomia acordada

Las capacidades y perfiles persistentes se agrupan por accion o configuracion
física, no por cercania accidental entre estados actuales.

| Capacidad | Estados que gobierna | Razon |
|---|---|---|
| `BodyDimensions` | Sensores y motores que dependen de la cápsula | Perfil físico de radio y alturas semánticas por actor. No concede acciones ni reemplaza el `Collider` de Avian. |
| `GroundMovement` | Walk, Sprint, Sneak, Stairs | Locomocion terrestre, incluida la adaptacion a tramos de escalera authored. Sus perfiles permiten distinto tuning por actor sin convertir Stairs en una habilidad separada. |
| `AirborneMovement` | Fall | Perfil base de fisica aerea para un actor sujeto a gravedad. No concede una accion de jugador, pero permite variar por actor la caida y el control aereo. |
| `ClimbMovement` | Climb | Escalar una superficie, con velocidad, stamina y adherencia configurables. Implementada. |
| `LedgeTraversal` | Mantle, AutoVault | Maniobras de borde. Un actor puede trepar un borde bajo sin poder sostenerse en una pared. Implementada. |
| `WallJumpMovement` | WallJump, EdgeLeap | Rebote desde un contacto de pared. Debe poder existir sin `ClimbMovement`, para soportar la regla estilo Metroid. Implementada para las reglas actuales. |
| `LadderMovement` | Ladder | Interaccion authored, restringida a su linea y sin gasto de stamina. Implementada. |
| `JumpMovement` | Jump | Salto basico configurable; una entidad terrestre puede carecer de el. Implementada. |
| `GlideMovement` | Glide | Planeo configurable; no se deriva de estar en el aire. Implementada. |

No toda configuracion persistente es una habilidad opcional:
`AirborneMovement` es el perfil base que debe recibir cualquier actor sujeto a
gravedad y `BodyDimensions` describe su cuerpo de cápsula. Las acciones
opcionales como Jump o Glide se mantienen separadas para que su ausencia siga
teniendo significado de gameplay.

Los perfiles físicos de sensing siguen la misma distinción: `GroundSensing`
describe el probe de suelo obligatorio para el núcleo cinemático y
`LedgeSensing` describe las consultas opcionales que producen `LedgeFacts`.
No conceden acciones ni se fusionan con una capacidad de motor.

## Reglas de frontera

`ClimbMovement` no es un permiso implicito para Mantle, WallJump, EdgeLeap ni
Ladder. Esas maniobras tienen origenes y reglas propias:

- Mantle ya puede seguir a Climb, Ladder o WallJump; AutoVault comienza desde
  suelo. Ambas pertenecen a una futura capacidad de borde.
- WallJump actual comienza desde Climb o Ladder. Su futura variante estilo
  Metroid requerira una capacidad propia y un hecho temporal de contacto
  reciente con pared, no un estado Climb artificial.
- El contacto reciente, sus ventanas temporales y las normales de pared son
  hechos/sensores o estado temporal por actor. No son capacidades persistentes.
- Ladder sigue siendo independiente de que la pared sea escalable.

## Orden de migracion

1. `GroundMovement`: Walk, Sprint, Sneak y Stairs. Completado y validado.
2. Migrar las capacidades de traversal ya estables sin cambiar reglas:
   `ClimbMovement`, `LedgeTraversal`, `WallJumpMovement` y
   `LadderMovement`. El ticket `movement-traversal-capabilities` las agrupa
   por solicitud explicita del usuario y conserva todos los valores Player.
   Completado y validado.
3. Disenar la variante estilo Metroid de `WallJumpMovement` y el contrato de
   contacto reciente con pared.
4. Migrar `JumpMovement`, `GlideMovement` y el perfil `stairs` de
   `GroundMovement` sin modificar reglas. `movement-air-and-stairs-capabilities`
   define ese corte. Completado y validado.
5. Migrar `AirborneMovement` para que Fall deje de depender de tuning global
   y conserve su papel de fallback base. Completado y validado.
6. Migrar `BodyDimensions` para que sensores y motores de cápsula no dependan
   de la geometría global del Player. Completado y validado.
7. Agrupar el contrato común del actor y el estado runtime de cada capacidad
   en bundles de construccion, sin convertir bundles en capacidades ni cambiar
   los `Query` de motores. Completado y validado.
8. Migrar los perfiles físicos de GroundService y LedgeService para que sus
   casts no dependan de constantes globales de Player. Completado y validado.

Cada paso debe mantener el arbitraje central, el orden del schedule y los
contratos `Intents`/facts. Un ticket no puede adelantar una frontera de los
pasos posteriores para resolver un caso conveniente.

## Ejemplos de composicion

- Link: `BodyDimensions` + `GroundMovement` + `AirborneMovement` +
  `ClimbMovement` + `LedgeTraversal` + `WallJumpMovement` + `LadderMovement`.
- Criatura humanoide: las mismas capacidades con perfiles de tuning distintos.
- Caballo: `BodyDimensions` grande + `GroundMovement` + `AirborneMovement`
  con perfiles rapidos; sin
  `ClimbMovement`.
- Personaje de plataforma: `GroundMovement` + `AirborneMovement` +
  `WallJumpMovement`, sin
  `ClimbMovement`.

Al construir esas entidades, `KinematicActorBundle` provee el núcleo común y
los bundles de capacidad agregan su tuning y estado privado. Son ergonomía de
spawn: no sustituyen los componentes de capacidad ni activan sistemas.

Los ejemplos describen composicion futura; no autorizan crear esas entidades
ni cambiar el comportamiento de los motores hasta sus tickets respectivos.
