# Rationale: por qué Health no decide nada de dominio (codex)

## El problema

`Health` lo necesitan Player, Enemies y potencialmente Mounts. Si vive
dentro de Combat (el sistema que más obviamente lo usa), Enemies y Mounts
tendrían que depender de un módulo interno de Combat para algo que no es
semánticamente de Combate — el mismo error de dirección de dependencia que
ya se corrigió para el núcleo de arbitración (`proposal-arbitration-core.md`)
y para el redirect de Monturas (`mounts-intent-redirect.md`).

## La decisión

`Health` es un sistema hermano, tan "tonto" como sea posible:

- Consume `DamageRequestMessage` y aplica el daño a un pool
  `{ current, max }` solo si el target tiene `Health`. Nada más. (codex)
- No sabe qué es sigilo, ni qué multiplica el daño — ese cálculo ya pasó en
  Combate (o en quien emita el mensaje) antes de que el monto le llegue.
  (codex)
- Si aplicó daño, emite `DamageAppliedMessage`. Los sistemas que reaccionan a
  daño recibido escuchan ese resultado, no el pedido crudo, para no reaccionar
  a daño descartado por target inválido. (codex)
- No decide qué pasa cuando `current` llega a 0 — emite `DeathMessage` y se
  desentiende. Loot, respawn o despawn son decisiones de quien posee la
  semántica del actor (Enemies para un enemigo, un sistema de flujo de
  partida para el jugador), no de Health. (codex)

Esto es la misma regla que ya aplica al núcleo de arbitración
(`proposal-arbitration-core.md`: "el núcleo compartido no puede conocer
estados concretos ni reglas de dominio") — aquí extendida a un pool de vida
en vez de una cola de propuestas.

## Por qué esto no es sobre-ingeniería

Es tentador meter `Health` directo dentro de `combat.rs` porque hoy solo
Combate lo usa activamente. Pero Enemies ya necesita leerlo (huida al estar
herido, GDD §7) y Projectiles ya necesita escribirle daño sin pasar por
Combate — dos consumidores reales, no hipotéticos, existen en el objetivo de
diseño. No es la abstracción prematura que la Constitución pide evitar (esa
regla aplica a diseñar para casos que *todavía* no existen). (codex)
