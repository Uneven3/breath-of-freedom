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

## Foco activo

**Preparar la fase de contenido** (pedido del usuario): agrandar el mundo y
sumar modelos/animales/personajes sin que el costo crezca con cada pieza.

1. **Mundo-como-datos, paso 2:** mover las tablas de `world/layout.rs` a un
   asset (RON o escena GLTF de Blender). El paso 1 (separar
   mecanismo/contenido) ya está — `layout.rs` es la costura.
2. **Animación por arquetipo:** al llegar el segundo modelo riggeado
   (horse o enemigo), generalizar `visuals/animation.rs` — mapeo
   estado→clip como datos por tipo de actor, no sistema por modelo.
3. **Mapear clips restantes del player:** Jump_Start/Loop/Land a los
   estados de salto; Sword_* al combate; Hit_Knockback al daño recibido.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **Facciones:** `Perceivable` es un bit; reemplazar por facción cuando
  haya hostilidad entre no-jugadores (animales, aliados).
- **Ítems/inventario:** no existe; llega con "cortar árboles" (la madera
  debe ir a alguna parte). El patrón destructible ya existe
  (`PracticeTarget` + `Health` + reacción del dueño en `world/`).
- **Lock-on de cámara** y **escudo/parry**: siguientes piezas de combate.
- **Durabilidad de armas:** depende de inventario.

## Decisiones que el usuario debe tomar pronto

- Formato de autoría del mundo: ¿tablas RON o escenas GLTF desde Blender?
  (define el pipeline de assets de todo lo que viene).
- Próximo modelo a integrar: ¿horse o enemigo? (dispara la generalización
  de animación).
