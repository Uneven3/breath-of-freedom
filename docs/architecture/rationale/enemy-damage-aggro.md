# Rationale: Reacción a Daño de Enemigos y Aggro Inmediato (antigravity)

**Estado:** superseded por `enemies::DirectThreatMessage`; se conserva la
motivación histórica. No existe `DamageAppliedMessage` en el código actual.

## El problema

En el diseño original de `enemies.md`, se planteaba que el sistema de percepción (`Perceive`) actualizaba los objetivos de agresión leyendo transforms y líneas de visión (ray-casting de campo de visión). 

Sin embargo, esto genera un problema de jugabilidad y realismo físico:
1. **Ataques Fuera de Visión:** Si el jugador ataca a un enemigo sigilosamente por la espalda (con arco o aproximación desde atrás), el ray-cast de línea de visión no lo detectará de inmediato porque queda fuera del cono de visión del enemigo.
2. **Falta de Reacción:** El enemigo recibiría daño y su vida bajaría (procesado por `Health`), pero no sabría *quién* lo atacó ni de dónde provino la agresión, quedándose inmóvil o en estado de alerta estático en lugar de reaccionar de inmediato agresivamente contra el atacante.

---

## La decisión

La implementación vigente usa un mensaje propiedad del receptor:

1. **Percepción Integrada por Mensajería:**
   Combat emite `enemies::DirectThreatMessage` al conectar una interacción no
   bloqueada; Enemies consume su propio contrato además de visión/oído.

2. **Gatillado de Reacción por Impacto:**
   Cuando se recibe `DirectThreatMessage` para el enemigo:
   * Se identifica al agresor (`source`).
   * Se actualiza la percepción de inmediato (`AggroTarget = Some(source)`), anulando la necesidad de que esté dentro de su cono de visión inicial.
   * Se fuerza la transición del estado de IA a `EnemyAiState::Combat` para iniciar la confrontación.

---

## Consecuencia

Los enemigos reaccionan de manera responsiva y coherente a los ataques del jugador (tanto a distancia como cuerpo a cuerpo), incluso si ocurren desde puntos ciegos. Esto evita tener que implementar lógicas complejas de "rayos de dolor" en el sistema de salud o en los proyectiles, manteniendo la simulación de IA desacoplada y guiada por eventos claros de resultado de simulación.
