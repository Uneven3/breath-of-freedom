# grass_02 — set PBR de terreno

Fuente de `assets/textures/terrain/T_GroundGrass_{Albedo,Normal}.png`, que son
copias renombradas byte por byte de `grass_02_base_1k.png` y
`grass_02_normal_gl_1k.png` (verificado por MD5 el 2026-07-26).

Estaba duplicado dentro de `assets/` (11 MB commiteados dos veces); movido acá
porque `assets/` es runtime y `art/vendor/` es donde el contrato de
`ASSET_PIPELINE.md` pone las fuentes de terceros.

**Licencia: SIN DECLARAR.** Los archivos llegaron al repo sin nota de origen y no
hay forma de deducirla desde el contenido. El contrato exige `bof_license` en
todo asset authored; esto es vendor, pero la obligación de conocer la procedencia
es la misma. **Completar antes de shipear**, o reemplazar por un set con licencia
conocida.

`normal_dx` (convención DirectX) no se usa: Bevy espera la convención OpenGL,
que es `normal_gl`. Se conserva por si el set se re-exporta.
