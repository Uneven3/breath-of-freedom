#!/usr/bin/env python3
"""Cuenta lo que hay en una captura del juego, sin adivinar nada.

Uso:
    python3 tools/shot_stats.py target/shots/grass.png [--bands 8]

# Qué reemplaza, y por qué el reemplazo no es opcional

El 2026-08-06 las decisiones sobre el pasto se tomaron con perfiles radiales por
**detección de bordes** sobre la imagen bonita. Ese método tiene dos defectos que
no se ven en su salida: satura con densidad alta —a partir de cierto punto todo
es borde y el número deja de moverse— y no distingue una brizna baja de la
ausencia de brizna. Por eso no vio el galón de briznas a media altura, que era
justamente el artefacto que se estaba buscando.

Acá no hay detección de nada. El juego pinta cada anillo de un color plano y
exacto (`grass-view=medir`), escribe al lado del PNG qué color es cada cosa, y
esto **cuenta píxeles iguales a esos colores**. Es aritmética: no hay umbral que
elegir, no satura, y una brizna a media altura son menos píxeles de su color,
no un borde menos.

# El analizador no conoce ningún color

Los lee del `.json` que la corrida escribió al lado de la foto. Un color
hardcodeado acá sería la misma clase de bug que este trabajo vino a cerrar: dos
copias del mismo dato, y sólo una se actualiza.

# Sólo la biblioteca estándar

Ni numpy ni Pillow. Una herramienta que no corre en otra máquina es una
herramienta que no sirve, y este repo ya decodifica PNG a mano en
`generate_terrain_textures.py`.
"""

from __future__ import annotations

import json
import struct
import sys
import zlib
from pathlib import Path


#: Cuánto puede inclinarse la cámara antes de que "fila de pantalla" deje de
#: significar "distancia". 0,25 son unos 15°, que es donde el orden todavía se
#: conserva en el rango que el pasto ocupa.
HORIZON_TOLERANCE = 0.25


def read_png(path: Path) -> tuple[int, int, bytes, int]:
    """Devuelve (ancho, alto, píxeles, canales) de un PNG de 8 bits sin entrelazar."""
    raw = path.read_bytes()
    if raw[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path} no es un PNG")

    width = height = 0
    channels = 0
    idat = bytearray()
    offset = 8
    while offset < len(raw):
        (length,) = struct.unpack(">I", raw[offset : offset + 4])
        kind = raw[offset + 4 : offset + 8]
        body = raw[offset + 8 : offset + 8 + length]
        offset += 12 + length  # longitud + tipo + datos + CRC
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8:
                raise SystemExit(f"{path}: sólo 8 bits por canal, no {depth}")
            if interlace:
                raise SystemExit(f"{path}: entrelazado no soportado")
            channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(colour, 0)
            if channels == 0:
                raise SystemExit(f"{path}: tipo de color {colour} no soportado")
        elif kind == b"IDAT":
            idat += body
        elif kind == b"IEND":
            break

    data = zlib.decompress(bytes(idat))
    stride = width * channels
    out = bytearray(height * stride)
    previous = bytearray(stride)
    pos = 0
    for row in range(height):
        filter_kind = data[pos]
        pos += 1
        line = bytearray(data[pos : pos + stride])
        pos += stride
        # Los cinco filtros del formato. Están acá enteros a propósito: un
        # decodificador que sólo entiende el filtro 0 funciona hasta el día en
        # que el codificador cambia de humor, y falla en silencio con basura.
        for i in range(stride):
            left = line[i - channels] if i >= channels else 0
            up = previous[i]
            up_left = previous[i - channels] if i >= channels else 0
            if filter_kind == 1:
                line[i] = (line[i] + left) & 0xFF
            elif filter_kind == 2:
                line[i] = (line[i] + up) & 0xFF
            elif filter_kind == 3:
                line[i] = (line[i] + ((left + up) >> 1)) & 0xFF
            elif filter_kind == 4:
                p = left + up - up_left
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - up_left)
                nearest = left if (pa <= pb and pa <= pc) else (up if pb <= pc else up_left)
                line[i] = (line[i] + nearest) & 0xFF
            elif filter_kind != 0:
                raise SystemExit(f"{path}: filtro {filter_kind} desconocido")
        out[row * stride : (row + 1) * stride] = line
        previous = line
    return width, height, bytes(out), channels


def load_legend(png: Path) -> dict | None:
    sidecar = png.with_suffix(".json")
    if not sidecar.exists():
        return None
    return json.loads(sidecar.read_text())


def describe(width: int, height: int, pixels: bytes, channels: int) -> None:
    """Luminancia, dispersión y saturación medias de la imagen.

    **Dos capturas del mismo encuadre, o esto no compara nada.** Cambiar de
    mirador cambia todos estos números por razones que no tienen que ver con lo
    que se estaba probando.
    """
    total = width * height
    lum_sum = lum_sq = sat_sum = 0.0
    for i in range(0, total * channels, channels):
        r, g, b = pixels[i], pixels[i + 1], pixels[i + 2]
        # Rec. 601, que es la que usa el ojo para "más claro / más oscuro".
        lum = 0.299 * r + 0.587 * g + 0.114 * b
        lum_sum += lum
        lum_sq += lum * lum
        top, bottom = max(r, g, b), min(r, g, b)
        sat_sum += 0.0 if top == 0 else (top - bottom) / top
    mean = lum_sum / total
    # Varianza por el momento de segundo orden; con enteros de 8 bits y un
    # millón de muestras la cancelación catastrófica no llega a importar.
    sd = max(lum_sq / total - mean * mean, 0.0) ** 0.5
    print(f"  luminancia media {mean:6.1f}   desviación {sd:5.1f}   saturación {sat_sum / total:5.1%}")


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    png = Path(argv[1])
    bands = 8
    if "--bands" in argv:
        bands = int(argv[argv.index("--bands") + 1])

    width, height, pixels, channels = read_png(png)
    legend = load_legend(png)

    print(f"{png}  {width}x{height}  {channels} canales")

    if legend is None:
        # Sin leyenda no se puede contar por categoría, pero sí describir la
        # imagen. Es lo que sirve para comparar dos versiones del **mismo
        # encuadre**: la desviación por canal separa "cada brizna tiene su tono"
        # de "el campo es un verde liso", que a ojo se discute y no se zanja.
        print(
            "sin leyenda al lado (.json): no hay conteo por categoría.\n"
            "Para eso, sacala con BOF_KNOBS=grass-view=5. Mientras tanto, la imagen:"
        )
        describe(width, height, pixels, channels)
        return 0

    view = legend.get("vista", "?")
    print(f"vista: {view}")
    # Qué vistas pintan plano y exacto. Sale de la leyenda —la corrida sabe cuáles
    # son— y no de una lista acá, que se desactualizaría en cuanto haya otra.
    if not legend.get("plana", False):
        print(
            "AVISO: esta vista no pinta colores planos y exactos. La luz y la niebla\n"
            "mueven cada píxel, así que este conteo no significa nada."
        )

    wanted = {tuple(item["color"]): item for item in legend["categorias"]}
    total = width * height
    counts = {tuple(item["color"]): 0 for item in legend["categorias"]}
    band_counts = [dict.fromkeys(counts, 0) for _ in range(bands)]

    # Las filas por banda se cuentan, no se estiman: dividir por `alto/bandas`
    # cuando las bandas no son todas del mismo tamaño da porcentajes por encima
    # del 100%, que es imposible para una cobertura exclusiva. Salió así en la
    # primera corrida — 100,7% — y un medidor que puede pasarse del 100% no se
    # puede usar para nada.
    band_rows = [0] * bands
    for y in range(height):
        band = min(y * bands // height, bands - 1)
        band_rows[band] += 1
        row = y * width * channels
        for x in range(width):
            i = row + x * channels
            key = (pixels[i], pixels[i + 1], pixels[i + 2])
            if key in counts:
                counts[key] += 1
                band_counts[band][key] += 1

    grass = sum(counts.values())
    print(f"\ncobertura de pradera: {grass / total:6.2%} de la pantalla ({grass} px)")
    print(f"{'categoría':<34} {'color':>9} {'px':>10} {'pantalla':>9} {'del pasto':>10}")
    for colour, item in wanted.items():
        n = counts[colour]
        share_of_grass = n / grass if grass else 0.0
        print(
            f"{item['nombre']:<34} {'#%02X%02X%02X' % colour:>9} "
            f"{n:>10} {n / total:>8.2%} {share_of_grass:>9.1%}"
        )

    # El perfil por bandas horizontales reemplaza al perfil radial: con la cámara
    # mirando al horizonte, la fila de pantalla crece monótonamente con la
    # distancia al suelo, así que esto **es** el perfil de cobertura contra
    # distancia — sin detectar un solo borde.
    #
    # **Y "con la cámara mirando al horizonte" es una condición, no una frase.**
    # Inclinada hacia abajo la fila deja de ordenar por distancia y el perfil
    # pasa a describir algo que no existe. La corrida escribe su pose al lado de
    # la foto justamente para que esto se pueda verificar en vez de suponer.
    facing = (legend.get("camara") or {}).get("facing")
    if facing is None:
        print(
            "\nAVISO: la leyenda no trae la pose de la cámara, así que no se puede\n"
            "verificar que la fila de pantalla ordene por distancia. Perfil omitido."
        )
        return 0
    if abs(facing[1]) > HORIZON_TOLERANCE:
        print(
            f"\nAVISO: la cámara mira con dy={facing[1]:+.2f}, demasiado inclinada.\n"
            "La fila de pantalla ya no crece con la distancia, así que un perfil por\n"
            "bandas describiría algo que no existe. Perfil omitido a propósito."
        )
        return 0
    print(f"\nperfil por bandas (arriba = lejos), {bands} bandas; columnas en el orden de arriba:")
    header = (
        "  banda "
        + " ".join(f"{index:>7}" for index in range(len(counts)))
        + "    total"
    )
    print(header)
    for index, band in enumerate(band_counts):
        pixels_in_band = band_rows[index] * width
        if pixels_in_band == 0:
            continue
        band_total = sum(band.values())
        cells = " ".join(f"{band[c] / pixels_in_band:>6.1%}" for c in counts)
        print(f"  {index:>5} {cells} {band_total / pixels_in_band:>8.1%}")

    framing = legend.get("encuadre", {})
    if framing:
        print(
            f"\nen cuadro: {framing.get('mallas_visibles')} mallas, "
            f"{framing.get('triangulos')} triángulos, {framing.get('draws')} draws · "
            f"pradera {framing.get('pradera_triangulos')} tris en "
            f"{framing.get('pradera_draws')} draws"
        )
        meadow_tris = framing.get("pradera_triangulos") or 0
        if grass and meadow_tris:
            print(f"triángulos de pradera por píxel cubierto: {meadow_tris / grass:.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
