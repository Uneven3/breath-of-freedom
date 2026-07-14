# Ticket: `input-camera-foundation`

## Sistema(s)

Input, Movement y Camera. No se pueden separar: Movement hoy lee hardware y
`CameraRig.yaw`, mientras Camera es quien muta ese yaw. El ticket mueve ambos
a un contrato que permite control local, IA y red sin que los motores conozcan
la fuente de input.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md`.
2. `docs/ARCHITECTURE-MAP.md` (Input, Movement y Camera).
3. `docs/COUPLING-MAP.md`.
4. `docs/architecture/input.md`.
5. `docs/architecture/movement.md`.
6. `docs/architecture/camera.md`.
7. `docs/architecture/rationale/fixed-update-input-gating.md`.
8. `docs/architecture/rationale/multi-actor-dispatch.md`.

## Acoplamiento

- Input -> Movement: `READ` + `WRITE-OWN`. Input publica acciones; Movement
  escribe solamente sus propios `Intents` y cursor de consumo.
- Input -> Camera: `READ`. Camera consume orientación de control y no escribe
  simulación.
- Camera -> Movement: `READ`. Camera sigue un actor local, sin ser fuente de
  orientación ni input de simulación.
- Enemies y Multiplayer siguen fuera de alcance, pero el contrato debe dejar
  un `InputSource` por actor para que los mismos brains sirvan en el futuro.

## Alcance

- Crear `src/input/` con acciones de capacidad fija, snapshots por fuente y
  binding local por defecto resuelto en `PreUpdate`.
- Añadir `InputControlledBy`, `ControlOrientation` y estado/cursor de climb
  por actor.
- Migrar `movement::brain` para leer acciones resueltas, no hardware ni
  `CameraRig`.
- Migrar Camera para leer orientación de control y seguir al actor local.
- Actualizar `src/main.rs`, `src/movement/mod.rs` y docs de Input/Movement/
  Camera/mapas si el contrato real difiere.
- Hacer opt-in el trace de debug y eliminar allocations por tick de los logs
  de transición.

## Fuera de alcance

- UI de rebinding, persistencia de bindings, gamepad y chord complejos.
- `ApplyRemoteActionsMessage`, transporte multiplayer y Brain de IA.
- Combate, restricciones locomotoras y el dispatcher de los 13 motores.
- Cambios de feeling en locomoción ya validados.

## Definición de terminado

- [x] Ningún módulo de gameplay lee `ButtonInput<KeyCode>`.
- [x] El actor local se enlaza a un `InputSource`; la orientación de control y
      el latch de climb son componentes por actor, no resources globales.
- [x] Movement no depende de Camera; Camera no escribe ningún dato de
      simulación de Movement.
- [x] Las acciones sostenidas y los gatillos sobreviven correctamente entre
      `PreUpdate` y múltiples ticks de `FixedUpdate`.
- [x] Debug no aloca ni captura casts/proposals en `FixedUpdate` por defecto.
- [x] Tests cubren aislamiento de fuentes, consumo único de gatillos y el
      contrato actor/cámara; `cargo fmt`, clippy y tests pasan.

## Notas

El usuario confirmó que `Intents` debe poder venir de control local, red o
IA. El núcleo se implementa ahora; red/IA usarán el mismo enlace por fuente
sin que Movement conozca sus tipos.

## Resultado

- `src/input/` publica snapshots de acciones de capacidad fija por
  `InputSource`. Los adaptadores de red pueden actualizar esos snapshots sin
  depender de `ButtonInput`; los brains de IA pueden omitir
  `InputControlledBy` y escribir sus propios `Intents`.
- `ControlOrientation`, `InputConsumeCursor` y `ClimbInputState` viven en el
  actor. Camera solo lee la orientación; Movement no importa Camera.
- La instrumentación de propuestas reutiliza `ProposalBuffer`; el trace y los
  casts se mantienen apagados por defecto.
