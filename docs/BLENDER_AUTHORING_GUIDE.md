# Guía personal: modificar mesh low-poly, riggear y animar (Blender → GLB → Bevy)

> **Ayuda personal, NO documentación del proyecto.** Este archivo está sin
> trackear a propósito (no forma parte del repo ni del presupuesto documental).
> El contrato autoritativo del pipeline vive en `ASSET_PIPELINE.md`. Acá va el
> "cómo hacerlo en Blender" paso a paso, en criollo.

El objetivo típico: tomar un mesh low-poly (uno de Quaternius, o el maniquí), 
ajustarlo, ponerle esqueleto, animarlo con los nombres `AN_<Rol>` que el juego 
espera, y exportarlo a GLB. La decisión más importante primero:

**¿Rig nuevo o reusar el rig del maniquí?** Para reemplazar al player, **reusá el
esqueleto del maniquí UAL** (mismos nombres de hueso). Así todas las animaciones
UAL1/UAL2 y las tuyas enganchan sin retargeting. Riggear desde cero sólo si es un
asset que no comparte locomoción con el player (un NPC raro, una criatura).

---

## 0. Preparar la escena (una sola vez, siempre)

Esto tiene que coincidir con el pipeline o el asset entra torcido/gigante:

1. **Unidades**: `Scene Properties` (ícono cono+esfera) → `Units` → Unit System
   `Metric`, Unit Scale `1.0`. **1 unidad = 1 metro.** Un personaje mide ~1.8 m.
2. **Orientación**: en Blender el frente del asset mira a `-Y` y `+Z` es arriba.
   El exportador convierte a la convención Bevy (`-Z` frente, `+Y` arriba). No
   pelees con esto: modelás mirando `-Y`.
3. **Pivote/origen**: para un personaje, el origen va **entre los pies**, en el
   piso. (`Object` → `Set Origin` → `Origin to 3D Cursor` con el cursor en 0,0,0).
4. **Aplicar transforms** antes de exportar/riggear: seleccioná el mesh →
   `Ctrl+A` → `All Transforms` (rotation + scale a 1, location limpia). Un mesh
   con escala ≠ 1 arruina el rig y el export.

---

## 1. Modificar el mesh low-poly

1. **Importar** el asset base: `File` → `Import` → `glTF 2.0` (los Quaternius y el
   maniquí son `.glb`/`.gltf`).
2. **Entrar a Edit Mode**: seleccioná el mesh, `Tab`. Vértices `1`, aristas `2`,
   caras `3`.
3. **Editar manteniéndolo low-poly** (esta es tu dirección estética):
   - Mover/escalar/rotar selección: `G` / `S` / `R` (+ `X`/`Y`/`Z` para eje).
   - Extruir: `E`. Bevel: `Ctrl+B`. Loop cut: `Ctrl+R`. Disolver: `X` → `Dissolve`.
   - **No metas subdivisiones ni smooth shading pesado.** Menos tris, silueta
     legible. El watchdog del juego avisa si un mesh pasa ~2000 tris por parte.
4. **Materiales**: pocos, planos. `Material Properties` → un `Principled BSDF` con
   `Base Color` sólido, `Metallic 0`, `Roughness` alto. En el pipeline los
   materiales se nombran `M_<Cosa>` (ej. `M_Bark`). Menos materiales = menos draw
   calls.
5. **UVs**: si cambiaste geometría con textura, re-desplegá (`U` → `Smart UV
   Project` en Edit Mode). Para colores planos por cara alcanza con material por
   isla, sin textura.
6. **Normales**: si ves caras negras/invertidas, Edit Mode → seleccioná todo
   (`A`) → `Shift+N` (recalcular hacia afuera).
7. Volvé a Object Mode (`Tab`) y **aplicá transforms** (`Ctrl+A` → All).

---

## 2. Riggear (poner esqueleto)

### Opción A — Reusar el rig del maniquí (recomendado para el player)

Es la ruta plug-and-play: mismo esqueleto → las 86 animaciones UAL enganchan.

1. Importá el maniquí UAL (`UAL1_Standard.glb`). Trae `Armature` + mesh
   `Mannequin`. Borrá el mesh `Mannequin`, quedate con el `Armature`.
2. Poné tu mesh en la **misma pose de bind (T-pose)** que el esqueleto, mismo
   tamaño y posición (los pies del mesh sobre los pies del rig).
3. Seleccioná **primero el mesh, después el Armature** (Shift+click, el armature
   queda activo/resaltado).
4. `Ctrl+P` → **`With Automatic Weights`**. Blender liga el mesh al esqueleto y
   pinta pesos automáticos. Ahora mover un hueso mueve el mesh.
5. **Probar y corregir pesos**: entrá al Armature en `Pose Mode` (`Ctrl+Tab`),
   rotá huesos (`R`) y mirá deformaciones feas (ej. hombro que arrastra el torso).
   Para arreglar: seleccioná el mesh → `Weight Paint` mode → pincel (rojo = 1,
   azul = 0) sobre el vertex group del hueso problemático. `Ctrl+Z` es tu amigo.
   Volvé la pose a cero antes de seguir (`Alt+R`, `Alt+G`).

### Opción B — Rig nuevo desde cero (NPCs/criaturas propias)

1. `Add` → `Armature`. Entrá a `Edit Mode` del armature.
2. Extruí huesos (`E`) formando la jerarquía: raíz → pelvis → columna → cuello →
   cabeza, y cadenas de brazos/piernas. Nombrá los huesos con claridad
   (`spine_01`, `arm_l`, …); los nombres son parte del contrato de animación.
3. Volvé a Object Mode, seleccioná mesh + armature, `Ctrl+P` → `With Automatic
   Weights`. Corregí pesos igual que arriba.
4. (Opcional) **Rigify** (`Edit` → `Preferences` → `Add-ons` → activar "Rigify")
   te da un metarig humanoide listo para editar — más rápido que hueso por hueso.

**Nombres de objetos** (importa para el export): la raíz del asset se llama
`ROOT_<asset_key>`, el mesh renderizable `SK_<Parte>_LOD0` (skinned) con sus
`_LOD1`/`_LOD2` si hacés niveles de detalle. Ver `ASSET_PIPELINE.md`.

---

## 3. Añadir animaciones

Las animaciones son **Actions** en Blender. El nombre de la Action **es** el
contrato: el juego busca `AN_<Rol>` (ver la tabla en `ASSET_PIPELINE.md`).

### Setup del área de trabajo

- Abrí un editor **`Dope Sheet`** y cambialo a modo **`Action Editor`** (dropdown
  arriba). Ahí creás/nombrás/cambiás Actions.
- Otro editor útil: **`Timeline`** (abajo) para play/scrub y setear el rango de
  frames (Start/End).

### Crear una animación (ej. `AN_Walk`)

1. En el `Action Editor`, click **`New`** → renombrala **exactamente**
   `AN_Walk` (case-sensitive, prefijo `AN_`, `PascalCase` en la acción).
2. **Activá `Fake User`** (el ícono del escudo 🛡 al lado del nombre). Sin esto,
   Blender borra la Action al cerrar si no está "usada". Crítico.
3. Poné el frame en `Timeline` (ej. frame 1). Entrá al Armature en **`Pose
   Mode`**. Posá los huesos (`R`/`G`) para el primer frame del ciclo.
4. **Insertá keyframe**: seleccioná los huesos posados (`A` para todos) → `I` →
   `Location & Rotation` (o `Whole Character` si querés todo). Aparece la llave.
5. Avanzá frames (ej. a 12), cambiá la pose, `I` de nuevo. Repetí hasta armar el
   ciclo (ej. contacto-paso-contacto-paso).
6. Ajustá el **rango**: Start/End en el Timeline para que el clip dure sólo lo del
   ciclo.

### Reglas que el juego necesita (no negociables)

- **In-place (sin root motion):** el juego mueve al personaje con física. La
  animación **no** debe llevar traslación horizontal en la raíz/cadera, o el
  personaje "patina" o se va de largo. La cadera puede subir/bajar (bob), pero en
  XZ se queda en el lugar. (Por eso usamos el GLB `UAL1_Standard`, no el `_RM`.)
- **Loops sin costura** (Walk/Run/Idle/Crouch/Fall/Glide): el **último frame debe
  empatar con el primero**. Truco: copiá la pose del frame 1 al último frame + 1,
  y exportá el rango sin ese último frame duplicado. Si al reproducir hay un
  "salto", la costura está mal.
- **One-shot** (Jump/Mantle/Vault/Climb): juegan una vez, no loop.
- **Un solo Armature** por asset, con los nombres de hueso del rig del maniquí si
  compartís locomoción.

### Qué clips autorar (prioridad para el player)

Empezá por los que faltan y se ven todo el tiempo. El contrato completo está en
`ASSET_PIPELINE.md`; los **más urgentes** hoy son los de strafe, porque el juego
ya los pide y caen a walk:

- `AN_WalkStrafeL`, `AN_WalkStrafeR`, `AN_WalkBwd`
- `AN_RunStrafeL/R/Bwd`, `AN_SneakStrafeL/R/Bwd`
- (los base `AN_Idle/Walk/Run/Sneak/Jump/Fall/...` sólo si hacés personaje propio
  desde cero — con el rig UAL reusado ya los tenés).

Consejo: `AN_WalkStrafeR` es a menudo `AN_WalkStrafeL` espejado (`Ctrl+C`/`Ctrl+V`
de poses con `Paste Flipped` en Pose Mode).

---

## 4. Exportar a GLB

Usá el exportador del proyecto (`tools/export_blender_asset.py`) para que salga
con las convenciones; o exportá a mano así y después pasalo por el pipeline:

`File` → `Export` → `glTF 2.0 (.glb)`, con:

- **Format**: `glTF Binary (.glb)`.
- **Include** → `Selected Objects` (seleccioná armature + meshes antes).
- **Transform** → `+Y Up` activado.
- **Data → Mesh**: `Apply Modifiers`, UVs, Normals.
- **Data → Armature**: `Export Deformation Bones only` (opcional, limpia huesos de
  control) — pero cuidado de no romper la jerarquía que el juego espera.
- **Animation**: activá `Animations`, `Export All Actions` (o las que marcaste con
  Fake User). Cada Action se exporta como un clip con su nombre.

Root motion: **desactivá** cualquier opción de sample/bake que introduzca
traslación de raíz si tu clip es in-place.

---

## 5. Validar (las tres redes de seguridad)

1. **Exportador** (`tools/export_blender_asset.py`): falla si los nombres no
   siguen la convención.
2. **Build** (`build.rs`): al poner el GLB en `assets/game/authored/` y declarar
   `bof_animset = "player"` en las custom properties de la raíz, el build
   **falla si falta un clip `required`**, nombrando cuál. Es tu guardrail: si
   escribís `AN_Wlak`, no compila.
3. **Runtime**: el juego liga cada rol; los clips que falten caen a su fallback y
   se loguea a `debug!`. Corré el juego y probá el clip con **F7** (navegador de
   animaciones) + `[` / `]` para cyclar.

---

## Atajos y errores comunes

- **El mesh no se mueve con los huesos** → no aplicaste `Ctrl+P With Automatic
  Weights`, o el mesh no tiene el modifier `Armature` apuntando al armature.
- **El personaje sale gigante/enano** → escala ≠ 1 sin aplicar (`Ctrl+A → Scale`),
  o unidades mal.
- **Se "patina" al caminar** → la animación tiene root motion; hacela in-place.
- **La animación desaparece al reabrir** → te faltó el `Fake User` (🛡).
- **Salto/pop en el loop** → primer y último frame no empatan.
- **Caras negras** → normales invertidas (`Shift+N` en Edit Mode).
- **Deformación fea en codo/hombro** → pesos; `Weight Paint` sobre ese vertex group.
- Guardá seguido (`Ctrl+S`) e incrementá versiones (`File` → `Save As` con `+`).
