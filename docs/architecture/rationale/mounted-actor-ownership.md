# Rationale: ownership de actor montado y lifecycle rider–mount

**Estado:** implementado; checkpoint jugado final pendiente.

## Problema

El horse debe ser un `Actor` de Movement para reutilizar locomoción, sensing y
stairs. Eso no implica que el rider y el horse deban ejecutar dos pipelines
físicos completos mientras Mounts corrige uno al final del tick.

El prototipo auditado dejaba al rider participando en Movement y luego Mounts
sobrescribía su `Transform`/`BodyVelocity`. También conservaba intents del
horse al desmontar, no garantizaba relación uno-a-uno, no ordenaba todas las
fronteras con Combat/Health y duplicaba cleanup entre E, F8 y muerte. El
refactor resolvió esos findings mediante los contratos siguientes.

## Ownership decidido

| Dominio | Posee |
|---|---|
| Movement | Escritura del cuerpo, suspensión locomotora, attachment cinemático, redirect y neutralización de `Intents`. |
| Mounts | Relación `MountedOn`/`RiddenBy`, selección de asiento, reglas de montar/desmontar, carga y consecuencias de muerte del mount. |
| Combat | Contexto/effective profile montado y snapshot de ataques. |
| Health/interaction | Validación de inmunidad por fuente bajo una semántica explícita. |
| Presentation | Mesh/material del horse, animación y cues; nunca escribe simulación. |

Mounts nunca escribe directamente el cuerpo de un `Actor`. Emite contratos
poseídos por Movement. Movement no importa tipos de Mounts.

## Contratos genéricos de Movement

Movement incorpora datos/mensajes genéricos reutilizables por vehículos,
plataformas móviles y compañeros:

- `KinematicAttachment`: actor portado, carrier y pose local.
- `LocomotionEnabled`: marker poseído por Movement; un actor adjunto conserva
  `Actor` para Combat/Camera, pero no participa en Sense/Gather/Arbitrate/Tick.
- Mensaje de attach: valida entidades, evita self/ciclos, deshabilita collider,
  suspende locomoción y registra la pose local.
- Mensaje de detach: aplica pose segura ya resuelta, hereda velocidad, reactiva
  collider, neutraliza intents y reactiva locomoción.
- Redirect persistente/release explícito: copia solo controles autorizados y
  limpia el controlled actor cuando termina la relación.

`brain::read_intents` sigue leyendo el controller suspendido. El redirect
transfiere control al horse. Desde SenseWorld en adelante, solo el horse tiene
locomoción habilitada. Después del motor del carrier, Movement sincroniza los
attachments y fija un borde explícito antes de Combat.

## Relación Mounts

Mounts mantiene ambos extremos:

- `MountedOn(mount)` en el rider.
- `RiddenBy(rider)` en el mount.

Un único sistema de transición valida y escribe la relación. Montar requiere
que ambos extremos estén libres. Desmontar, F8, muerte y orphan cleanup pasan
por el mismo lifecycle, de modo que nuevos campos no queden sin restaurar.

Si desaparece cualquiera de los extremos de forma inesperada, un sistema de
reconciliación detecta la relación inválida y pide a Movement un detach/release
seguro. Nunca queda `ColliderDisabled` ni redirect apuntando a una entidad
inexistente.

## Ordering objetivo

El schedule debe expresar todas las dependencias; los `Message`s no las crean:

```text
Mounts request/lifecycle
  -> Movement apply attachment/control changes
  -> Movement ReadIntents/Redirect/Sense/Gather/Arbitrate/Tick
  -> Movement SyncAttachments
  -> Combat context/read/gather/tick
  -> Projectiles
  -> Mounts charge emit
  -> Health apply
  -> death consequences (preparadas para lifecycle siguiente)
```

La muerte puede conservar el horse marcado como pendiente durante un tick si
eso permite detach antes de despawn. Se prefiere una latencia fija documentada
a una referencia colgante o un orden accidental.

## Desmontaje seguro

El desmontaje normal evalúa un conjunto fijo, sin allocations, de candidatos a
derecha/izquierda y alrededor del mount. Cada candidato valida:

- Overlap de la cápsula de pie.
- Headroom.
- Suelo alcanzable y pendiente permitida.
- Exclusión del rider y mount en la query.

Si no existe candidato, el desmontaje voluntario se rechaza y el rider sigue
montado. F8/muerte usan una política forzada distinta y documentada: búsqueda
radial fija y fallback libre de overlap desde el que el actor pueda caer. La
velocidad inicial hereda el planar del carrier.

## Combat montado

`WeaponProfile` base nunca se reemplaza al montar. Combat deriva un perfil
efectivo desde `CombatContext` y `MountedCombatProfile`. Al iniciar Windup o un
draw de arco se snapshottea el step/profile efectivo; cambiar contexto durante
una fase activa no modifica daño, alcance ni timing a mitad de acción.

Esto evita restaurar un arma a pie obsoleta cuando Equipment cambie y mantiene
Combat desacoplado de Mounts. El contexto se aplica antes de Combat y puede
identificar el tipo de mount sin importar su marker concreto.

## Owner e inmunidad

`mounted rider` y `owner` no son sinónimos. La relación persistente de owner
debe modelarse explícitamente o el contrato debe llamarse inmunidad al rider
actual. La decisión de producto vigente pide inmunidad total frente al owner.

La semántica implementada es inmunidad a toda interacción hostil. El contrato
se llama `HostileInteractionImmunity`: Combat, Projectiles y Mounts lo
consultan antes de HP, feedback, threat o impulso; Health repite la validación
autoritativa. No se conserva un nombre de “damage immunity” para suprimir
efectos que el tipo no promete.

## Debug y presentación

F8 se captura en Debug/Input y solo emite una solicitud. Spawn/despawn y
lifecycle se resuelven en `FixedUpdate`. Meshes/materiales/cues viven fuera de
la simulación y leen estado del horse.

## Invariantes

- Movement es el único writer del cuerpo de cualquier `Actor`.
- Un rider adjunto no ejecuta motores, pero puede conservar input/combat/camera.
- Cada rider tiene como máximo un mount y cada mount un rider.
- Attach/detach son simétricos y release neutraliza el horse.
- Combat siempre lee la pose de silla del tick actual.
- Charge emite antes de Health con latencia determinista.
- Ninguna desaparición deja relación, collider o redirect colgante.
- Ningún cambio de contexto altera una acción Combat ya iniciada.
