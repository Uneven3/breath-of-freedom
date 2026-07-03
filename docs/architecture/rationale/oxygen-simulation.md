# Rationale: Simulación y Recuperación de Oxígeno vía Sistema Global (antigravity)

## El problema

En el diseño original de `swim.md`, se propuso que el consumo y recuperación de `Oxygen` (para el estado locomotor `Dive`) se manejara de forma interna por el motor `Dive` dentro de `TickActiveMotor`. Esto es similar a cómo el motor `Sprint` o `Climb` drena `Stamina`.

Sin embargo, esto genera un problema de asimetría lógica:
1. **Recuperación fuera del agua:** El oxígeno debe recuperarse cuando el jugador está en superficie (`Swim`) o en tierra firme (`Walk`, `Sprint`, `Idle`, etc.).
2. **Ciclo de Ejecución de Motores:** En la arquitectura de movimiento, un motor *solo* ejecuta su función `tick()` si su `LocomotionState` correspondiente es el estado activo elegido por el árbitro.
3. **Acoplamiento de Motores Terrestres:** Si el oxígeno solo se recupera cuando el motor `Dive` no está activo, la lógica de recuperación de oxígeno tendría que inyectarse en los sistemas de actualización de todos los demás motores (Caminar, Sprintar, Trepar, Deslizarse, etc.), obligándolos a declarar dependencias mutables sobre `Oxygen`. Esto acopla fuertemente el subsistema de natación/buceo a todos los motores terrestres básicos.

---

## La decisión

Se delega la simulación de `Oxygen` a un sistema global único (`update_oxygen`) que se ejecuta en `FixedUpdate` para cualquier entidad que tenga el componente `Oxygen`:

1. **Desacoplamiento Absoluto de Motores Terrestres:**
   Los motores de locomoción terrestres y aéreos (`src/movement/motors/`) no necesitan saber que existe el oxígeno, ni declararlo en sus queries de consulta, manteniendo sus responsabilidades limpias y aisladas.

2. **Lógica Centralizada y Clara:**
   El sistema `update_oxygen` consulta el `LocomotionState` activo de cada actor:
   * Si el estado es `LocomotionState::Dive`, drena el aliento:
     ```rust
     oxygen.drain(OXYGEN_DRAIN_PER_SEC * dt);
     ```
   * En cualquier otro caso, recupera el aliento:
     ```rust
     oxygen.recover(OXYGEN_RECOVER_PER_SEC * dt);
     ```

3. **Mapeo de Daño Autoritativo:**
   Si la intensidad de `Oxygen` cae a `0.0` durante la inmersión, este mismo sistema emite `health::DamageRequestMessage` de tipo asfixia hacia el `target`. El motor `Dive::tick` solo se preocupa de la velocidad física 3D y de realizar la integración de fuerzas bajo el agua. (codex)

---

## Consecuencia

El sistema de natación y buceo se extiende de forma aditiva y limpia (Constitución §2). Ningún motor de movimiento preexistente tiene que ser modificado para soportar el ciclo de vida del oxígeno, y la lógica temporal de asfixia queda centralizada en un solo sistema determinista del host.
