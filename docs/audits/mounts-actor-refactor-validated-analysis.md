# Análisis validado — refactor Mounts sobre `Actor`

Fecha: 2026-07-16.

Este documento contrasta el informe frío archivado en
`docs/audits/mounts-actor-refactor-cold-audit.md` contra el código actual.
Preserva los problemas confirmados y los puntos que todavía requieren una
decisión de diseño. No propone ni fija soluciones.

## Conclusión

La puntuación 2/100 del informe frío es demasiado dramática como medida de
calidad general. Sin embargo, el veredicto `VIOLATION` sí corresponde: hay
incumplimientos explícitos de la Constitución y bugs reales de lifecycle,
ownership y scheduling. Los checks verdes prueban unidades locales, pero no
el comportamiento del schedule real.

## Problemas confirmados y prioritarios

### 1. El horse conserva sus últimos `Intents` al desmontar

Al desaparecer `MountedOn`, dejan de emitirse redirects, pero nadie escribe
`Intents::default()` en el horse. Como no tiene Brain propio, puede continuar
caminando, sprintando o cargando indefinidamente.

Evidencia: `src/movement/mod.rs::redirect_controls` y
`src/mounts/mod.rs::dismount`.

### 2. Ordering incompleto entre Mounts, Combat y Health

`sync_riders`, `charge_enemies` y `handle_interact` solo están ordenados
después de `MovementSet::TickActiveMotor`. Combat también está después de ese
set, pero no existe orden entre ambos plugins.

Consecuencias posibles:

- Combat lee la posición anterior del rider.
- `SetMountedCombatMessage` se aplica este tick o el siguiente.
- El daño de carga llega a Health este tick o el siguiente.

Los `Message`s de Bevy 0.19 no crean ordering por sí mismos.

### 3. Movement simula al rider y Mounts sobrescribe el resultado

El Player montado continúa participando en sensing, propuestas, arbitraje y
motor. Después, `sync_riders` reemplaza su `Transform` y `BodyVelocity`. La
colocación en silla puede pertenecer conceptualmente a Mounts, pero el diseño
actual deja un pipeline oculto avanzando y dos sistemas escribiendo el cuerpo.
Contradice Constitución §7 y el invariante de un único writer de movimiento.

### 4. Violación explícita de Constitución §19

`src/mounts/mod.rs` mezcla Components, Resources, configuración del plugin,
presentación, input, lifecycle, física, carga y tests. `src/combat/context.rs`
mezcla datos (`Component`/`Message`) con el sistema que los procesa.

### 5. El desmontaje no es físicamente seguro

`dismount` siempre teleporta al mismo lado a distancia fija. No valida overlap,
paredes, suelo, headroom ni el lado alternativo. Puede colocar al rider dentro
de geometría o sobre vacío, contradiciendo la documentación que lo llama
“safe dismount”.

### 6. El perfil de combate cambia durante ataques activos

`apply_mounted_context` reemplaza `WeaponProfile` sin consultar
`CombatState`. Montar o desmontar durante Windup/Active/Recovery puede cambiar
timings, alcance y daño a mitad del swing, o invalidar el índice actual de
combo.

### 7. `CombatContext` conserva una copia obsoleta del arma a pie

`on_foot_weapon` se inicializa con `GRAYBOX_SWORD`. Cuando Equipment cambie el
arma real, desmontar restaurará esa copia inicial y sobrescribirá lo equipado.
El problema existe aunque Equipment todavía no esté implementado.

### 8. El coste de stamina del salto ocurre antes del arbitraje

`jump::propose` drena stamina antes de saber si Jump ganó y antes de confirmar
que el proposal entró al buffer. Un actor genérico puede pagar por un salto
derrotado por WallJump, Mantle, EdgeLeap o AutoVault.

### 9. No existe una relación uno-a-uno rider–horse

Dos riders pueden montar el mismo horse. Movement procesaría ambos redirects,
neutralizaría ambos controllers y el último mensaje sobrescribiría los intents
del horse. Tampoco existe recuperación automática completa si el horse
desaparece por una ruta distinta de F8 o `DeathMessage`; el rider puede quedar
con `MountedOn` y `ColliderDisabled` colgantes.

### 10. El horse posee capacidad estructural de Sneak

`GroundMovement` agrupa Walk, Sprint y Sneak, y `GroundMovementBundle` agrega
los estados de crouch. El redirect convierte el input Sneak a Walk, pero la
composición contradice el contrato de que el horse no recibe esa capacidad.

### 11. F8 modifica simulación desde `Update`

`toggle_spawn` lee `ButtonInput<KeyCode>` y escribe transforms, velocidades y
componentes desde `Update`. Aunque sea una herramienta de debug, no está
encapsulada en Debug y contradice Constitución §20.

## Problemas válidos que requieren una decisión de diseño

### Inmunidad y consecuencias de impacto

Health, Combat y Projectiles duplican parte de la política de inmunidad. La
supresión completa de daño, feedback y knockback coincide con el feedback del
checkpoint jugado, pero `DamageSourceImmunity` solo promete inmunidad de daño.
Debe decidirse si el dominio real es “daño rechazado” o “interacción rechazada”.

### `owner` no está modelado

La inmunidad actual existe mientras está montado y se elimina al desmontar.
Eso representa inmunidad al rider actual, no inmunidad persistente frente al
dueño, como dicen los requisitos. Además, un único componente solo representa
una fuente inmune.

### Límite y detección de `HorseCharge`

`HorseCharge` recuerda 16 objetivos; el objetivo 17 se ignora y contradice
“cada Enemy una vez por carga”. También recorre todos los enemigos cada tick y
usa distancia entre centros en vez de overlap/sweep físico, por lo que puede
golpear a través de geometría y escalar mal en mundo abierto.

### Cursor de input compartido

Mounts usa el `InputConsumeCursor` de Movement. Hoy las acciones no compiten,
pero contradice el contrato de cursor por consumidor y permite interferencias
si los dominios comparten una acción futura.

### Simulación y presentación mezcladas

`MountAssets`, meshes y materiales viven dentro del mismo módulo/plugin que la
simulación de Mounts. Es una mezcla adicional contraria a Constitución §§1,
19 y 20.

## Documentación desactualizada

- `docs/architecture/combat.md` todavía afirma que Health no existe.
- `docs/ARCHITECTURE-MAP.md` dice que Combat sigue pendiente de migración
  multi-actor.
- `docs/architecture/input.md` declara `Parry`, ausente de `IntentAction`.
- El mapa afirma que `DamageAppliedMessage` ya se emite, pero sigue diferido.
- `assets/Prototype.glb` no está referenciado ni justificado.
- La documentación llama seguro a un desmontaje sin validación espacial.

## Huecos de testing

Los 152 tests pasan, pero los nuevos tests de Mounts usan `run_system_once` y
no validan:

- El schedule real de los plugins.
- Mount → redirect → dismount → siguiente fixed tick.
- Muerte mientras está montado.
- Desaparición inesperada del horse.
- Dos riders y un mismo horse.
- Cambio de perfil durante un ataque.
- Colocación de desmontaje con pared, borde y falta de headroom.
- Objetivo 17 de una carga.

## Control flow y rendimiento

No se encontró un problema central de `else if` anidado. Los casos existentes
son transiciones normales de timers/estados. El smell real es la duplicación
del lifecycle de desmontaje entre E, F8, horse inexistente y muerte, que hace
probable un cleanup incompleto.

No hay allocations evidentes por tick en `charge_enemies`,
`redirect_controls` o `sync_riders`; usan arrays y queries. Sí falta una prueba
de la invariante de no allocations establecida por Constitución §§11/18.

## Estado para la conversación de soluciones

Las soluciones objetivo quedaron documentadas después de esta auditoría en:

- `docs/architecture/rationale/movement-capability-composition.md`.
- `docs/architecture/rationale/mounted-actor-ownership.md`.
- `docs/implement-feature/movement-capabilities-and-mount-lifecycle-plan.md`.

La implementación todavía no comenzó. El plan resuelve primero contratos y
composición, luego ownership/lifecycle y finalmente combate, inmunidad, carga,
presentación, documentación y checkpoint.
