# Ticket: `diagonal-climb-continuation-normal`

## Diagnóstico

El trace mostró `climb=false/true side=true/true n=(0,0,0)` al aproximarse a
la pared en diagonal. Los rayos laterales no oscilaban: la clasificación de
continuación aceptaba la pared hasta 45°, pero `climb_normal` se publicaba
solo dentro del umbral estricto de 30° usado para iniciar Climb.

## Regla

- Iniciar Climb requiere perfil completo y ángulo de 30°.
- Fuera de Climb, la previsualización de continuidad permite 45°.
- Durante Climb, cualquier hit de cintura sobre superficie escalable mantiene
  la continuidad y publica su normal. La orientación ya la controla el motor;
  no se usa como condición para soltar una pared.

## Verificación

- [x] La normal se publica para la continuación válida, incluso con movimiento
      diagonal mientras Climb está activo.
- [ ] Play-test: una vez sujeto, subir y desplazarse en diagonal no pierde la
      normal ni los límites laterales.
