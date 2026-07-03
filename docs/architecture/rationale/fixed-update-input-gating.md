# Rationale: Resolución de Bindings en PreUpdate y Consumo de Gatillos en FixedUpdate (antigravity, codex)

## El problema

En el diseño inicial de `input.md`, se proponía que la resolución de combos y bindings de teclado/gamepad (`resolve_bindings`) corriera al inicio de `FixedUpdate` (antes de la fase `ReadIntents`). 

Este supuesto de ejecución en `FixedUpdate` acarrea dos bugs graves de integración con Bevy:
1. **Entradas Doblemente Gatilladas (Double Triggering):**
   `FixedUpdate` corre un número variable de veces (cero, una o múltiples veces) por frame de render para mantener la física a 60 Hz estables. Bevy actualiza y limpia el estado de las teclas presionadas en `PreUpdate` una vez por frame de render.
   Si en un frame de render lento `FixedUpdate` corre dos veces consecutivas para ponerse al día, el sistema `resolve_bindings` detectará la tecla presionada (ej. `just_pressed(KeyCode::Space)`) en **ambos** ticks. Esto causaría un doble salto o doble ataque inmediato en la simulación física a partir de una única pulsación física.
2. **Entradas Perdidas (Missed Inputs):**
   Si la tasa de refresco visual es muy alta (ej. 240 Hz) y `FixedUpdate` se salta en un frame en particular, el estado `just_pressed` de la tecla se establecerá y limpiará en la fase de render antes de que `FixedUpdate` se ejecute en el siguiente frame. La simulación nunca verá que la tecla fue presionada, perdiendo el input del jugador.

---

## La decisión

Se separa la **resolución física del hardware** de la **lectura de intenciones de la simulación**:

1. **Resolución en `PreUpdate` (Una vez por frame de render):**
   El sistema `resolve_bindings` se registra para ejecutarse en `PreUpdate`, inmediatamente después del sistema de actualización de input nativo de Bevy. De esta forma, lee el hardware exactamente en el momento correcto y una sola vez por frame visual.

2. **Diferenciación de Acciones en `ActiveActions`:**
   El recurso `ActiveActions` clasifica las acciones por `InputSource` en dos semánticas:
   * **Acciones de Estado (Sustained):** Consultadas mediante `.pressed(action)`. Se sobreescriben en cada frame de render (ej. WASD para moverse, Shift para correr).
   * **Acciones de Gatillo (Triggered):** Representadas como contador/generación por acción. Se incrementan en `PreUpdate` cuando la acción es pulsada y permanecen disponibles hasta que cada consumidor actualice su propio cursor. (codex)

3. **Consumo Autoritativo en `FixedUpdate` sin mutar el snapshot global:**
   Cuando los sistemas de `ReadIntents` en `FixedUpdate` mapean el input a
   `Intents`/`CombatIntents`/`InteractIntents`, comparan la generación de la
   acción con su propio `InputConsumeCursor` (`Movement`, `Combat`, `NPCs`,
   etc.). Si la generación es nueva, procesan el trigger y actualizan su
   cursor. (codex)
   * **Si `FixedUpdate` corre múltiples veces:** El primer tick avanza el
     cursor del consumidor; el segundo tick ve la misma generación y no la
     procesa otra vez. (codex)
   * **Si `FixedUpdate` se salta:** La generación queda en `ActiveActions`
     hasta el siguiente tick; ningún sistema la pierde por limpieza de Bevy.
     (codex)
   * **Si dos dominios leen acciones distintas:** No compiten por un único
     `ResMut<ActiveActions>` ni se roban triggers; cada uno muta solo su
     cursor propio. (codex)

---

## Consecuencia

Se garantiza una entrada física responsiva y determinista a través de
fluctuaciones de framerate, eliminando bugs comunes de dobles saltos o inputs
ignorados. Se preserva al mismo tiempo el desacoplamiento: los motores de
simulación leen intenciones puras y desacopladas del hardware, y la
paralelización ECS no queda bloqueada por un recurso global mutable de input.
(codex)
