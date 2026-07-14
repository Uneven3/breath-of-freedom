# Ticket: `ladder-attachment-traversal`

## Sistema(s)

Movement / World. Ladder es un motor de anclaje vertical sin stamina; no es
una variante de `Climb` ni de `Stairs`.

## Problemas observados

- El trigger permite entrar desde cualquier dirección y el motor conserva
  movimiento lateral.
- El ancla actual sitúa el centro de la cápsula dentro de la pared.
- Al llegar arriba aplica un impulso vertical, pero `Mantle` no acepta
  `Ladder` como estado de origen.
- El escenario de prueba monta la escalera sobre una pared escalable, por lo
  que Climb y Ladder compiten sobre la misma superficie.

## Contrato

- `LadderFacts` publica ancla para el **centro del cuerpo**, normal exterior,
  base y cima; World es dueño de esos datos authored.
- Ladder entra solo mediante el toggle de climb; una vez anclado solo admite
  velocidad vertical y mantiene al actor mirando la pared.
- El borde superior se resuelve con la propuesta existente de `Mantle`, pero
  únicamente cuando el jugador pulsa su acción manual. No hay impulso vertical
  especial ni transición automática.
- Jump desde Ladder usa el motor `WallJump`; la orientación anclada da el
  vector de retroceso aunque la pared sea `NonClimbable`.
- `NonClimbable` es un marcador de World que LedgeService consulta para
  bloquear solo Climb, sin bloquear Mantle/Vault ni acoplar el motor Ladder a
  los sensores de Climb.

## Fuera de alcance

- Salto lateral desde ladder, ladder móvil, stamina, animaciones y sonido.
- Rediseño de `Stairs`.

## Verificación

- [x] La entrada lateral no propone Ladder; solo el toggle sí.
- [x] El tick no genera velocidad X/Z y fija orientación/ancla.
- [x] Mantle solo gana al solicitarse manualmente en la cima.
- [x] La pared de prueba con ladder no puede iniciar Climb.
- [ ] Play-test: entrada, ancla, salida inferior y Mantle superior en la
      pared nueva.
- [ ] `cargo fmt`, tests y clippy pasan.
