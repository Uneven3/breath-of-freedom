# T_GrassMeadowCard_Albedo

SPDX-License-Identifier: GPL-3.0-or-later

Asset original creado para el laboratorio `Card mesh` el 2026-08-10 mediante
la herramienta de generación de imágenes del proyecto. No usa una fuente
externa. Es una mata de pasto estilizada, aislada sobre chroma magenta y luego
recortada a PNG RGBA con alpha recto; el fichero final es 512 × 512.

Su RGB se ajustó a `#529438`, el verde unlit de las referencias LOD0/LOD1 del
laboratorio, sin alterar el alpha. En la pradera el shader no lo usa como una
segunda iluminación: conserva el gradiente y luz de las briznas, y toma de la
imagen sólo esa variación de luminosidad y su máscara.

El alpha se ajustó por separado sólo mediante tres entrantes amplios tallados
desde el borde inferior de la silueta existente. No se activaron píxeles nuevos
y los bytes RGB no cambiaron en esa intervención.

Uso: carta instanciada de la pradera y laboratorio `Card mesh`. Es fuente PNG
RGBA de 512 × 512, sin cadena de mips todavía; la conversión runtime con mips
sigue bloqueada por el pipeline de texturas descrito en `docs/TEXTURES.md`.
