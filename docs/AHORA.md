# Ahora — el trabajo presente

Conversación de trabajo entre sesiones y agentes. Presupuesto: **≤500
líneas**; lo cerrado se borra (queda en git), no se acumula. Léelo antes de
continuar; actualízalo tras cada decisión aceptada, checkpoint jugado o
cambio de foco. Reglas en `ARCHITECTURE.md`, visión en `NORTE.md`.

## Cómo trabajar en este repo

- Validación mínima antes de terminar: `cargo fmt` + `cargo clippy
  --all-targets -- -D warnings` + `cargo test`.
- El feeling se valida jugando (checkpoint, §10): lanzar con
  `env -u WAYLAND_DISPLAY DISPLAY=:1 cargo run` en background para el
  usuario; al cerrar la sesión, leer el log filtrando
  `error|panic|took|destroyed` antes de reportar.
- Debug in-game: F1 colliders, F2 casts, F3-F5 logs, F6 probe, F7 navegador
  de clips de animación (`[`/`]` ciclan, nombre en HUD).
- Commits a `main`, mensajes convencionales, sin push sin pedido explícito.

## Estado (2026-07-17)

Jugable y validado: locomoción completa multi-actor (walk/sprint/sneak/
jump/glide/climb/ladder/mantle/vault/wall-jump/stairs), enemigos con
percepción gradual (melee + arquero), health/muerte/respawn, horse (montar
F8/E, carga con sweep, inmunidad de dueño), espada con combos, arco de dos
fases con carga Bannerlord, modelo del player `Prototype.glb` (18 clips,
navegador F7), mundo graybox como tablas de datos (`world/layout.rs`).

Auditoría adversarial de arquitectura (2026-07-17): 4 hallazgos reales, 4
corregidos el mismo día (input a PreUpdate, patrón CapacityPending
eliminado, `Perceivable`, test del veto ForbidSprint). 187 tests.

## Foco activo — exprimir el graybox (decisión del usuario, 2026-07-17)

Hecho (checkpoint jugado 2026-07-17, "se ve increíble"): **ciclo día/noche**
(`world/day_night.rs` — TimeOfDay a ritmo BotW, sol/luna visibles, cielo
estilizado, F9/F10 de debug) y **identidad visual toon** (`visuals/toon.rs`
+ `visuals/outline.rs` — bandas discretas mate, outlines de post-proceso
por profundidad/normales). Los actores aún usan StandardMaterial continuo
(sus sistemas de tint/flash lo mutan) — toonificarlos es deuda menor.

Hecho, **pendiente de checkpoint jugado** (implementado 2026-07-18, tests
verdes 209/209, clippy/fmt limpios; el playtest lo hace el usuario —
entorno pasó a Wayland nativo, ver [[playtest-loop-launch-x11]]):
**inventario** (`src/inventory/` — arma equipable con swap/durabilidad,
materiales/comida apilables, pickup mixto: `Interact` para armas del
suelo, auto-collect para materiales/comida). Equipar inserta/retira
`WeaponProfile` (el componente ES el booleano de armado, tal como ya lo
anotaba `combat/weapon.rs`); romper el arma emite `WeaponBrokeMessage` y
desarma; `IntentAction::CycleWeapon` (tecla 4) re-equipa la primera arma
guardada; `IntentAction::UseItem` (tecla C) come el primer alimento y pide
`health::HealRequestMessage`. Pickups graybox en `world/layout.rs`:
`SpareClub` (Interact), `WoodPile`/`Apple` (auto), cerca del spawn.

Queda, en orden sugerido:

1. **Temperatura** — StatusEffects: zonas frías/calientes + exposición por
   hora del día (`TimeOfDay` ya existe) → `DamageRequestMessage` a Health;
   HUD graybox. Mitigación por equipo ya es viable (Inventory existe).
2. **IA de combate** — flanqueo, reacciones grupales, huida al estar
   herido (los enemigos ya leen su propio `Health`). Slice jugable sobre
   los brains existentes.

Pendiente sin fecha: mapear clips restantes del player (Jump_*, Sword_*,
Hit_Knockback); toon en actores; FXAA (MSAA quedó off por el outline).

## Estacionado — pipeline de assets (cuando termine la etapa graybox)

Recomendación investigada (2026-07-17, fuentes en git): Blender → glTF con
custom properties leídas vía `GltfExtras` de primera parte (sin Blenvy, que
está alpha/estancado); RON solo para datos no-espaciales; USD se ignora
(pipelines AAA). El editor oficial de Bevy se construye sobre BSN (0.19
solo código; archivos `.bsn` futuros) — la inversión Blender/glTF migra
limpio. `world/layout.rs` es la costura donde se enchufa.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **Facciones:** `Perceivable` es un bit; reemplazar por facción cuando
  haya hostilidad entre no-jugadores (animales, aliados).
- **Cortar árboles → madera real:** `Inventory`/`ItemKind::Material` ya
  existen; falta la mecánica de tala en sí (el patrón destructible ya
  existe: `PracticeTarget` + `Health` + reacción del dueño en `world/`).
- **Lock-on de cámara** y **escudo/parry**: siguientes piezas de combate.
- **Durabilidad de arco y de la espada montada:** fuera de alcance del
  inventario — ninguna pasa por un `WeaponDurability` equipable
  (`combat/context.rs::effective_weapon` sustituye la espada por
  `MOUNTED_SWORD` sin tocar Inventory; las flechas son un recurso aparte).
- **`combat::motors::attack::ProposeQuery` requiere `WeaponProfile` no
  opcional:** romper el arma a pie también bloquea el combate montado
  hasta re-equipar (quirk aceptado al agregar durabilidad).
- **Árbitro único de interactuables:** `MountInputCursor` e
  `InventoryInputCursor` consumen `Interact` en paralelo — un caballo y un
  arma ambos en rango disparan los dos sistemas con un solo `E`. Hoy no se
  dispara (los pickups del graybox están a ~8m del spawn del caballo); sin
  árbitro central, el primer layout que los acerque dispara ambos con una
  sola tecla.
- **Respawn no restaura arma:** si el jugador muere desarmado (arma rota)
  sin repuesto en `Inventory` ni un arma cercana en el mundo, respawnea
  con HP completo pero sin `WeaponProfile` — incapaz de atacar cuerpo a
  cuerpo hasta encontrar otra arma. `player.rs::respawn_on_death` no lo
  toca a propósito hoy (el inventario sobrevive a la muerte); decidir si
  el respawn debe garantizar un arma mínima.
- **`InventorySet` y `MountsSet::PostMove` sin orden explícito entre sí:**
  comparten banda (`.after(SyncAttachments).before(ApplyContext)`) sobre
  componentes hoy disjuntos; el primer feature que cruce ambos dominios
  (alforjas de caballo, loot al desmontar) hereda un orden no declarado.
- **`read_interact_pickups` duplica la selección de "candidato más
  cercano" de `mounts::lifecycle::read_interact_requests`** (filter +
  `min_by`/`distance_squared`) en vez de un helper compartido — un tercer
  sistema contextual (diálogo, campfire) copiaría por tercera vez.
- **Apilado de comida por igualdad exacta de `f32`:** `ItemKind::Food`
  apila por `PartialEq` derivado; una fuente futura que calcule `heal` en
  runtime (en vez de reusar un const) puede fallar el apilado por
  redondeo.

## Decisiones que el usuario debe tomar pronto

- Orden de ataque de la lista graybox (sugerido: día/noche → temperatura →
  inventario, con toon shader e IA de combate paralelizables).
