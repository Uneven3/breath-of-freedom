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

Antes de assets/pipeline, hay sistemas enteros que se construyen sin arte:

1. **Ciclo día/noche** — `TimeOfDay` en World (sustrato: no lee a nadie);
   sol rotando + luz/ambiente. Baratísimo y desbloquea temperatura e IA
   nocturna.
2. **Temperatura** — StatusEffects: zonas frías/calientes + exposición por
   hora del día → `DamageRequestMessage` a Health; HUD graybox. Mitigación
   por equipo llega después (necesita inventario).
3. **Inventario** — fundación de durabilidad, crafteo, loot de árboles y
   equipo térmico. Pieza arquitectónica grande: modelo de datos primero,
   UI graybox mínima después.
4. **Toon shader** — cel-shading (norte visual); material extension de
   Bevy, presentación pura, paralelizable con lo demás.
5. **IA de combate** — flanqueo, reacciones grupales, huida al estar
   herido (los enemigos ya leen su propio `Health`). Slice jugable sobre
   los brains existentes.

Pendiente sin fecha: mapear clips restantes del player (Jump_*, Sword_*,
Hit_Knockback) a estados reales.

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
- **Ítems/inventario:** no existe; llega con "cortar árboles" (la madera
  debe ir a alguna parte). El patrón destructible ya existe
  (`PracticeTarget` + `Health` + reacción del dueño en `world/`).
- **Lock-on de cámara** y **escudo/parry**: siguientes piezas de combate.
- **Durabilidad de armas:** depende de inventario.

## Decisiones que el usuario debe tomar pronto

- Orden de ataque de la lista graybox (sugerido: día/noche → temperatura →
  inventario, con toon shader e IA de combate paralelizables).
