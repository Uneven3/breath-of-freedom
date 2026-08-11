# Auditoría de lectura — boundaries entre capas (2026-08-11)

Auditoría de solo lectura (sin tocar código) sobre las leyes de
`ARCHITECTURE.md` que **el compilador y `tests/architecture.rs` no cobran**.
Antes de auditar a mano se corrió `cargo test --test architecture`: **los 15
tests pasan** (medido) — Cargo.toml de `bof_domain`/`bof_simulation` sin
`bevy`/`bevy_render`/Avian completos, presentación nunca nombra
`bof_simulation`, C2 (deuda de hardware congelada en 12 archivos), un solo
plugin raíz de simulación, cero material fuera del registro, cero `unsafe`,
techo de comentario del 30%. Esta auditoría no repite eso: cubre lo que solo
se ve leyendo.

## Resultado en una frase

Los boundaries **grandes** (crate↔crate, presentación↔simulación) están
sanos y verificados. Lo que aparece es más chico: superficie pública más
ancha de lo necesario dentro de un crate (§4) y una pieza de código de
simulación que cruzó las 1000 líneas (§16) sin quedar anotada como deuda.

## Hallazgos

### 1. §4 — superficie pública más ancha de lo necesario

- **`crates/simulation/src/movement/motor_common.rs`** — las ~11
  funciones/consts públicas del archivo (`move_toward`,
  `apply_locomotion_rotation`, `body_move_and_slide`, `snap_to_ground`,
  `align_with_floor`, `ground_drive_step`, `clip_below_ledge_lip`,
  `launch_normal`, `FLOOR_MIN_UP_DOT`, `LEDGE_TOP_OFFSET`,
  `GroundDriveStep`) tienen **cero** referencias fuera de
  `crates/simulation` — se usan solo entre motores hermanos
  (`movement/motors/{glide,fall,stairs,sprint,walk,sneak,jump,climb}.rs`).
  Cruzan frontera de *módulo*, no de *crate*: candidatos limpios a
  `pub(crate)`. El caso más sistemático encontrado, y de bajo riesgo —nada
  externo los usa, así que achicarlos no puede romper nada fuera del crate.
- **`crates/domain/src/perf.rs:57`** — `pub const SHADOW_DISTANCE_STEPS: [f32; 5]`
  es la única de doce constantes `*_STEPS` del archivo sin ninguna
  referencia fuera de `perf.rs`. Candidata a `pub(crate)`.
- **Falso positivo descartado:** `WeaponDurability::item()` **sí** cruza a
  `bof_simulation`: `inventory::equip::apply_equip_requests` lo usa para
  devolver al inventario el arma saliente con su durabilidad restante. Debe
  seguir siendo `pub`; el primer grep no reconoció la llamada por UFCS
  (`.map(WeaponDurability::item)`).

Fuera de estos dos, la superficie pública se ve disciplinada — los `pub`
grandes restantes (`inventory.rs`, `movement/abilities.rs`,
`Terrain`/`TerrainSnapshot`) cruzan de verdad a `src/` o son tipos que
simulación y presentación comparten legítimamente.

### 2. §16 — código de simulación que cruzó 1000 líneas sin quedar anotado

`AHORA.md` ya declara `grass.rs` y `perf/shot.rs` como deuda §16 conocida
(y sus cifras están algo desactualizadas: `shot.rs` 987 líneas hoy vs. 983
declaradas, `grass.rs` 1861 vs. 1605 — creció 16% desde que se documentó,
sigue siendo la misma deuda, solo el número es viejo).

Lo que **no** está anotado en ningún lado: `crates/simulation/src/world/terrain/mod.rs`
llegó a **1001 líneas**. Es el único de este tamaño que es código de
*simulación*, no visuales — el proyecto viene aceptando deuda de tamaño a
propósito en el lado visual, pero acá cruzó la marca de 1000 sin dejar
rastro escrito. Otros candidatos más chicos, sin urgencia: `src/perf/sequence.rs`
(1227), `crates/domain/src/perf.rs` (781), `src/perf/shot_stats.rs` (701),
`crates/simulation/src/combat/motors/aim.rs` (628).

### 3-9. Consistentes — sin acción

- **Mapa de módulos** (`ARCHITECTURE.md` líneas ~182-198): los 12
  directorios de `src/` están todos cubiertos por la tabla; nada huérfano.
- **§20, presentación como lectora pura, en la práctica** (no solo
  "no nombra el crate"): revisado todo `Query<&mut …>`/`Commands` en
  `src/camera/`, `src/visuals/`, `src/presentation/`, `src/sfx/`,
  `src/debug/` — todo lo mutado es presentacional propio (`Transform`,
  `Visibility`, VFX/UI propios). Ningún `Commands::insert` toca un tipo de
  dominio/simulación. `src/camera/` no linda con gameplay.
- **C1 (allocation en `FixedUpdate`)**: sigue vigente tal cual la describe
  `AHORA.md` — `crates/simulation/src/world/terrain/mod.rs:262-266`
  (`to_collider`) sigue armando `Vec<Vec<f32>>` que Avian reaplana, llamado
  desde `rebuild_terrain_collider` en `FixedUpdate`, gateado por
  `ColliderRevision`. No es un doc obsoleto.
- **113 ambigüedades de `FixedUpdate`**: `crates/simulation/src/lib.rs:244`
  sigue en `const FIXED_UPDATE_AMBIGUITIES: usize = 113`, con test de
  igualdad exacta (no solo `<=`) que pasó en esta corrida. Coincide con lo
  declarado.
- **Árbitro central único** (§7): `&mut LocomotionState` solo se escribe en
  `movement/spike.rs:189` (`arbitrate`), `&mut CombatState` solo en
  `combat/mod.rs:98` (`arbitrate`). Sin otros escritores.
- **`bof_domain` no importa `bof_simulation`**: confirmado, ni en
  `Cargo.toml` ni en el código.
- **"Malla + collider en la misma función" sigue exclusivo de
  `world::layout`/`world::spawn`**: no se filtró a ningún otro módulo.

## Prioridad de lo que vale la pena corregir

**Revisión crítica (2026-08-11, segunda pasada):** los 5 ítems de abajo no
son igual de "problema". Verificados a mano símbolo por símbolo (grep
directo, sin subagente), solo **1 y 2 son violaciones reales de un
boundary** — mecánicas, literales, de §4, riesgo cero de arreglar. El resto
son hallazgos válidos pero de otra categoría, y se reclasifican para no
tratarlos con la misma urgencia:

1. **`movement/motor_common.rs` → `pub(crate)`** (§4, violación real de
   boundary). Confirmado símbolo por símbolo: los 11 (`move_toward`,
   `apply_locomotion_rotation`, `body_move_and_slide`, `snap_to_ground`,
   `align_with_floor`, `ground_drive_step`, `clip_below_ledge_lip`,
   `launch_normal`, `FLOOR_MIN_UP_DOT`, `LEDGE_TOP_OFFSET`,
   `GroundDriveStep`) tienen cero apariciones en `src/` (el binario). Cruzan
   frontera de módulo, no de crate. Sin ambigüedad, sin riesgo.
2. **`perf.rs:57` (`SHADOW_DISTANCE_STEPS`) → `pub(crate)`** (§4, mismo
   tipo de violación real, confirmado).
3. *(Falso positivo descartado):* **`WeaponDurability::item()`**
   (`inventory.rs:232`) sí tiene consumidor en `bof_simulation` por UFCS
   (`.map(WeaponDurability::item)` en `inventory/equip.rs:40`); preserva la
   durabilidad al intercambiar armas. No cambiar su visibilidad ni borrarlo.
4. *(Reclasificado, no es un boundary):* **`terrain/mod.rs` (1001
   líneas) sin anotar como deuda §16.** Verificado que **no** mezcla
   responsabilidades: es un solo `impl Terrain` (línea 216→1001, 785
   líneas) con un método por pincel, exactamente el patrón intencional que
   `AHORA.md` documenta ("un séptimo pincel es un método + una fila, nunca
   un sistema nuevo"). Es grande porque es el único dueño de "cómo cambia
   la grilla", no porque cruce capas mal. §16 dice explícitamente "señal,
   no bloqueo" — bajar prioridad, no ignorar.
5. *(Reclasificado, cosmético, no un problema de código):* **cifras de
   líneas desactualizadas** en `AHORA.md`/`BOTWGrass.md` (983→987 en
   `shot.rs`, 1605→1861 en `grass.rs`) — es drift de documentación, no una
   falla arquitectónica. Vale la pena en algún momento, sin urgencia.

## Seguimiento de esta sesión

Las dos violaciones de §4 se corrigieron: los once símbolos de
`movement/motor_common.rs` y `SHADOW_DISTANCE_STEPS` ahora son `pub(crate)`.
El compilador y la batería completa de tests confirmaron que ningún consumidor
externo los necesitaba. `WeaponDurability::item()` permanece público por su
consumidor real en simulación. El tamaño de `terrain/mod.rs` sigue como señal
de §16, no como una división urgente.

## Archivos leídos

`docs/ARCHITECTURE.md`, `tests/architecture.rs`, `Cargo.toml` (raíz,
domain, simulation), y bajo grep/lectura dirigida:
`crates/domain/src/{perf,inventory}.rs`,
`crates/simulation/src/movement/motor_common.rs`,
`crates/simulation/src/movement/spike.rs`,
`crates/simulation/src/combat/mod.rs`,
`crates/simulation/src/world/terrain/mod.rs`, `crates/simulation/src/lib.rs`,
`src/camera/`, `src/visuals/`, `src/presentation/`, `src/sfx/`,
`src/debug/`, `src/world/layout.rs`, `src/world/spawn.rs`, más un barrido de
`wc -l` sobre `src/`, `crates/domain/src/`, `crates/simulation/src/`.
