# Rationale: LOD de sensing por distancia

`SenseWorld` dispara varios casts por actor por tick fijo (GroundService: 1
shape-cast; LedgeService: 6 shape-casts de perfil + 2 down-casts + 2 rays
laterales; más el sensor de stand-clearance de Sneak). Con un actor es
gratis; con un campamento de enemigos lejos del jugador es el costo dominante
del pipeline — y nadie puede ver parpadear los facts de un actor a 80 m.

## Decisión

Un componente `SensingLod` por actor y un sistema `lod::assign_sensing_lod`
que corre cada tick entre `ReadIntents` y `SenseWorld`:

- Dentro de `SensingLodConfig::full_rate_radius` del jugador local (o si el
  actor **es** el jugador): tier `Full`, sensa cada tick.
- Fuera del radio: tier `Reduced`, sensa una vez cada
  `SensingLodConfig::reduced_interval` ticks, **escalonado por índice de
  entidad** para que N enemigos repartan sus casts en la ventana en lugar de
  reventar el mismo tick.
- Sin jugador en el mundo (tests, headless): todos `Full` — default seguro.

Los servicios consultan `Option<&SensingLod>` y saltean al actor cuando no le
toca; un actor sin el componente sensa siempre. En un tick salteado los
`*Facts` conservan su último valor: la desactualización está acotada por el
intervalo (a 60 Hz e intervalo 4, ≤ 66 ms).

`SensingLodConfig` es un `Resource` con defaults documentados en el tipo —
es tuning de escala de mundo/encuentro, pensado para ajustarse por juego, no
constantes dispersas.

## Qué NO decide

- No apaga motores ni proposals: un actor `Reduced` sigue tickeando su motor
  cada frame con facts un poco viejos. "Dormir" actores enteros (skip de
  propose/tick fuera de un radio mayor) es una capa futura de Enemies, no de
  Movement.
- No reduce los casts del propio movimiento (`move_and_slide`,
  `snap_to_ground`): eso es costo del tick del motor, no de sensing.

## Por qué así

- El límite Facts/motores ya existía: los motores leen `*Facts` sin saber
  cuándo se escribieron, así que bajar la frecuencia de escritura no requirió
  tocar ni un motor.
- La decisión (1 booleano por actor por tick) se calcula una vez en un solo
  sistema, en lugar de repetir la geometría de distancias en cada servicio.
- El stagger usa `entity.index()` — estable frame a frame para la misma
  entidad, gratis, y suficientemente disperso entre entidades vecinas.

Ver `docs/architecture/movement.md` § Sistemas.
