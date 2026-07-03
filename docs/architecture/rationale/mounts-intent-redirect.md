# Rationale: Traducir Intents del Jinete a la Montura vía Sistema (antigravity)

## El problema

Mientras el jugador está montado, ¿de quién es el `Intents` que mueve el cuerpo: del jugador o de la montura? ¿Cómo se transfiere este input de manera desacoplada y compatible con multiplayer?

## La decisión de diseño original (Rechazada)

Originalmente se propuso que `brain::read_intents` (la frontera hardware → simulación) revisara si el jugador tiene el componente `MountedOn(Entity)`. Si era así, redirigía la escritura y escribía `MountIntents` directamente en la entidad de la montura en lugar de escribir `Intents` en el jugador.

### Por qué se descartó la propuesta original:

1. **Incompatibilidad con Multiplayer:**
   En una sesión multiplayer, el host corre la simulación física autoritativa
   de todos los actores (locales y remotos). Los actores remotos reciben
   `ActiveActions` desde `LocalInputFrame` en un `InputSource` de red; luego
   los mismos Brains genéricos escriben `Intents` en su actor. Si la
   redirección a `MountIntents` ocurre dentro de `brain::read_intents`, se
   mezcla Movement con Mounts y se abre una ruta especial que no aplica a IA,
   red u otros productores de `Intents`. (codex)

2. **Acoplamiento Circular (Violación de Constitución §14):**
   Para hacer la redirección en `brain::read_intents` (que pertenece al plugin de `Movement`), este debe importar `MountIntents` de `Mounts`. Esto genera un acoplamiento estrecho y dependencias circulares donde el core del movimiento necesita conocer los detalles del sistema de monturas.

---

## La nueva decisión: Sistema de Traducción Intermedio

En su lugar, las acciones resueltas siempre fluyen de manera uniforme al
componente `Intents` de la entidad que origina la acción mediante los Brains
genéricos (sea desde un `InputSource` local, un `InputSource` de red o IA).
(codex)

Luego, un sistema intermedio de traducción administrado por el plugin de **Mounts** (ej. `translate_mount_intents`) se ejecuta en cada tick de `FixedUpdate`:

```rust
// Se ejecuta en el mismo frame físico (FixedUpdate), después de leer inputs 
// y antes de que los motores de monturas procesen sus propuestas.
app.add_systems(
    FixedUpdate, 
    translate_mount_intents
        .after(MovementSet::ReadIntents)
        .before(MountSet::GatherProposals)
);
```

Este sistema realiza una query sobre los actores que tienen `Intents` y `MountedOn(mount_entity)`, traduce la dirección y deseos del jinete, y escribe en el componente `MountIntents` de la montura correspondiente.

### Ventajas de este enfoque:

* **Sin latencia adicional de frame local:** Al ordenarse de manera determinista dentro del mismo frame físico (`after(ReadIntents).before(GatherProposals)`), la montura reacciona en el **mismo frame** en que esa máquina aplica el input recibido. La suposición de que un sistema intermedio introduce latencia de frames es **falsa** gracias al scheduler de Bevy.
* **Uniformidad:** Funciona con cualquier productor de `Intents`: Brains
  genéricos alimentados por Input local/red o sistemas de IA. En v1
  multiplayer, el host es quien aplica esos inputs a la simulación
  autoritativa. (codex)
* **Desacoplamiento Estricto:** El plugin de `Movement` no sabe que existen las monturas. Es el plugin de `Mounts` el que depende de `Movement` (lee `Intents`), lo cual es la dirección de dependencia lógica correcta.

---

## Decisiones de diseño físico

* **Física del Jinete:** Desactivar colisiones del jinete (`Collider` / `RigidBody`) y emparentarlo visualmente a la montura mientras está montado.
* **Desmontar en Movimiento:** Determinar el estado físico inicial del jinete (ej. velocidad residual) si desmonta a mitad de un salto o planeo de la montura.
