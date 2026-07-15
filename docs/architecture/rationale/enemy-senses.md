# Rationale: el modelo de sentidos de los enemigos

Cómo "ve" (y oye, y siente el daño) una entidad IA. Nació orgánicamente en
los tickets `enemy-awareness` y `enemy-hearing-damage-aggro`; este doc fija
el modelo para que los sentidos futuros (olfato de un guardián, visión
nocturna, `TimeOfDay`) se agreguen sin redisenar.

## Un solo medidor, muchos estímulos

`Awareness` (`0.0..=1.0`, en `enemies/perception.rs`) es **la** conciencia
del enemigo: un escalar que no sabe de dónde vienen sus puntos. Cada sentido
es solo una *fuente de llenado* con su propia forma, velocidad y techo. El
brain (`decide`) y el futuro Combate leen únicamente los umbrales
(`SUSPICIOUS` = 0.35 investiga, `ALERTED` = 1.0 full threat) — agregar un
sentido nuevo no los toca.

No confundir los dos ejes del modelo:

- **Geometría del estímulo** (¿puede percibirte?): binaria e instantánea —
  cono de visión, radio de oído.
- **Detección** (¿cuánto te ha notado?): continua y temporal — el medidor.

No hay "campo de detección" como segunda geometría: emerge de que la tasa de
llenado escala con la distancia dentro del cono (`close_range_boost`). Cerca,
el medidor llena tan rápido que *se comporta* como detección instantánea.

## Los tres estímulos implementados

| | Vista | Oído | Daño (`DirectThreatMessage`) |
|---|---|---|---|
| Forma | Cono direccional (`fov_deg`, `sight_range`) | **Radio omnidireccional** (`hearing_range × loudness`) — la espalda no existe | Dirigido a un enemigo concreto |
| Oclusión | Pared = ciego total (ray a `GameLayer::Default`) | Pared = atenúa (`wall_muffle`), no bloquea | No aplica |
| Modulación | Cercanía (`close_range_boost`) y gait del target (`sneak_visibility`) | Loudness por gait del target: Sprint 1.0 > Walk 0.55 > Sneak 0.15; **quieto = silencio** | Ninguna: recibir daño no es ambiguo |
| Techo | `ALERTED` — solo la vista completa la detección | **`SUSPICIOUS`** — un ruido te hace girar e investigar, no te da full threat | Directo a `ALERTED`, salta el medidor |
| Información | Posición exacta, continua (`last_seen` sigue al target) | Posición del ruido, puntual | Posición de la amenaza |

Interacciones que **emergen** sin código propio:

- "No puedes matarlo por la espalda si te vio": por la espalda estás fuera
  del cono, el medidor nunca llenó — el ángulo no necesita regla.
- Ruido a la espalda → sospecha → `Search` hacia el ruido → **al girarse, la
  vista toma el relevo** y puede completar la detección. La tensión de
  sigilo (sneak lento y silencioso vs sprint rápido y ruidoso) sale del
  cruce de `sneak_visibility` × `loudness`.
- Flecha desde el sigilo → `ALERTED` sin verte → `decide` produce `Search` a
  full awareness (corre a investigar; Combate ya no dará sneakstrike). Si te
  encuentra, `Alert` directo.
- El ruido **sostiene** un medidor alto (no lo baja al techo de sospecha):
  un enemigo que te perdió de vista no se calma mientras te oiga.

## Decisiones de acoplamiento

- La loudness se **deriva read-only** del `LocomotionState` + `BodyVelocity`
  del target, dentro de Enemies: Movement no sabe que emite ruido, no hay
  canal nuevo. `presentation::cues` se descartó como fuente: son
  presentación (sin posición, derivados para SFX/VFX) — alimentar simulación
  desde ahí invertiría su capa.
- `DirectThreatMessage` es propiedad de Enemies y lo emitirán Health/Combat
  (patrón mensaje-del-receptor, como `health::DamageRequestMessage`). Hasta
  entonces solo lo ejercitan los tests — Debug no puede emitirlo (Debug es
  read-only sobre simulación).

## Extensiones previstas

- Ruidos discretos con intensidad propia (aterrizajes, ollas rotas, silbido
  para atraer): mismo medidor, probablemente sí como mensaje posicional.
- `TimeOfDay` modulando `sight_range` (noche = ver menos, GDD §10).
- Alerta grupal: un enemigo `ALERTED` emitiendo un estímulo audible para su
  `Faction` — otra vez, solo otra fuente de llenado.
