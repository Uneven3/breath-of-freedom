# Rationale: dos idiomas para estado transitorio por entidad

Surgido de comparar esta base con `lightcore` (match-3 en Bevy del mismo
autor): ambos juegos usan ECS idéntico en capacidades/hechos/tags, pero
representan el *estado transitorio por entidad* con idiomas distintos. Los
dos idiomas son válidos; cada invariante pide el suyo. Esta guía fija cuál
usar acá y cuándo.

## Los dos idiomas

**Estado-por-enum** (el nuestro para locomoción): un componente permanente
cuyo valor es el estado (`LocomotionState`). Entrar/salir = escribir el
valor.

**Estado-por-presencia** (dominante en lightcore): insertar/quitar un
componente marker es entrar/salir del estado (`Dropping`, `Selected`,
`PopAnim(Timer)`).

| | Presencia | Enum |
|---|---|---|
| Exclusividad | No garantizada — nada impide dos markers contradictorios | Garantizada por el tipo |
| Composición | Gratis — estados ortogonales conviven | Requiere componentes aparte |
| Filtrado | Por archetype: `With<X>` | Guard/`match` sobre el valor |
| Costo de transición | Archetype move | Escribir el valor |

## Regla de decisión

- Si la **exclusividad mutua es el invariante** (un motor por cuerpo, un
  arma activa, una animación de locomoción): **enum**, un solo escritor
  (Constitución §6/§7, `movement/state.rs`: "mutually-exclusive states are
  an enum, never a boolean soup").
- Si el estado es **ortogonal y componible** (agachado durante Walk *o*
  Stairs, un latch de stamina, un buff): **componente aparte** —
  presencia o con datos según necesite. Ya lo hacemos: `Crouched`,
  `SprintLock`, `SneakLock` viven fuera del enum justamente por esto.
- Un booleano dentro de un componente persistente que un sistema reescribe
  cada frame es una señal de que ese dato es un **hecho** y pertenece a
  `*Facts`, no al componente de capacidad/estado.

## Estados de app vs estados de simulación

`lightcore` fusiona en un solo `GameState` global sus pantallas de app
(`MainMenu`, `Options`, `Paused`) con las fases de simulación del tablero
(`SwapAnimating → Popping → Falling → …`). Funciona porque ahí todo es
global y exclusivo, pero acopla UI con simulación.

Acá esa fusión está prohibida por diseño: cuando este juego necesite
menús/pausa/título, será un `States` de Bevy **separado y chico**
(`MainMenu / Playing / Paused`), sin fases de gameplay adentro — la
simulación ya tiene su estado donde corresponde, por actor
(`LocomotionState`) y por sistema (sets encadenados en `FixedUpdate`).
`movement/state.rs` ya anticipa la frontera: Bevy `States` es "perfect for
app screens, wrong for per-entity locomotion".

## Capacidad = tag + datos

Una capacidad sin tuning degenera en tag puro (en lightcore `FallPhysics`
no tiene campos porque todas las piezas caen igual). Acá las capacidades
llevan tuning (`ClimbMovement` tiene velocidades y costos), pero el
mecanismo es el mismo: `With<ClimbMovement>` la usa como tag,
`&ClimbMovement` como datos. No agregar un `enabled: bool` a una capacidad
— quitar/insertar el componente *es* el booleano.
