# Rationale: Restringir el Recorte de Salto (Jump Cut) a Saltos de Jugador (antigravity)

## El problema

En el motor de caída (`src/movement/motors/fall.rs`), existe una lógica de "recorte de salto" (jump cut) que reduce la velocidad vertical del jugador a `JUMP_CUT_VELOCITY` (2.0) si el jugador suelta el botón de salto en pleno ascenso. Esto se hace para permitir saltos cortos o largos según la duración de la pulsación física.

Sin embargo, dado que `Fall` es el estado aéreo por defecto (al que se entra inmediatamente en el segundo frame del arco de salto y en cualquier caída libre):
* Si el jugador es lanzado hacia arriba por una fuerza externa (como una plataforma de salto, una explosión, una corriente de viento, o el ataque de un enemigo).
* Y en ese frame no está presionando el botón de salto (que es el caso habitual ante impactos inesperados).
* **El motor de caída recortaba instantáneamente su velocidad vertical a 2.0 en el primer tick.** El jugador nunca experimentaba el impulso completo de lanzamiento aéreo.

---

## La decisión

Se introduce el componente `JumpPhase` (`src/movement/motors/jump.rs`, L31-L36) en la entidad del jugador, el cual almacena un flag `is_player_jump: bool`.

1. **Activación:** Se establece en `true` en el motor de salto (`jump::propose`) en el frame exacto en que se inicia un salto voluntario del jugador.
2. **Uso:** El motor de caída (`fall::tick`) ahora valida que `jump_phase.is_player_jump` sea `true` antes de aplicar el recorte de velocidad:
   ```rust
   if jump_phase.is_player_jump && !intents.wants_jump && v.y > JUMP_CUT_VELOCITY {
       v.y = JUMP_CUT_VELOCITY;
   }
   ```
3. **Desactivación:** Se resetea a `false` en `jump::propose` cuando el actor vuelve a estar en el suelo (`on_floor`) o cuando transiciona a cualquier estado de locomoción que no pertenezca al arco aéreo primario (`Jump` o `Fall`), como agarrarse a un muro (`Climb`), planear (`Glide`), escalar una escalera (`Ladder`), o repisas (`Mantle`/`AutoVault`).

---

## Consecuencia

El control fino de altura de salto (pulsación corta/larga) sigue funcionando de manera idéntica para los saltos regulares del jugador, pero cualquier otra fuerza o velocidad vertical ascendente de origen externo es respetada plenamente, eliminando un bug crítico de físicas de juego y evitando recortes erróneos después de escalar o planear.
