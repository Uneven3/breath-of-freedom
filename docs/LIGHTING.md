# Iluminación y atmósfera — dirección técnica y parámetros

Hoja de ruta y especificación del sistema objetivo de iluminación, ciclo
día/noche, sombras, antorchas/interiores y atmósfera.

> **Cómo se usa este documento.** Es la referencia autoritativa de lo que se
> quiere construir para el ciclo día/noche, antorchas, sombras, niebla y clima.
> El estado vivo se consulta en código y `AHORA.md`; acá quedan decisiones,
> parámetros objetivo y criterios de aceptación. Cualquier cambio de
> iluminación se mide con el hub F1.
>
> **Filosofía (§ NORTE.md).** La belleza de este juego es **luz + color + atmósfera**,
> no detalle geométrico ni texturas complejas. Iluminación PBR estilizada sobre
> `StandardMaterial` plano y mate (`perceptual_roughness: 0.9`, `metallic: 0.0`).

---

## Lo que se ve y qué lo produce

| Lo que se ve | Técnica que lo produce | Por qué |
|---|---|---|
| El sol nace coral y muere magenta | Paleta de colores separada para Dawn (`SUN_DAWN_COLOR`) y Dusk (`SUN_DUSK_COLOR`) | Le da identidad cromática distinta a la mañana y la tarde |
| La noche no es negra ni aburrida | Luna direccional a 400 lux + ambiente azul frío (`brightness = 40.0`) | La noche se mantiene navegable sin aplanar el contraste |
| Antorchas y fogatas iluminan cálidamente los campamentos | `PointLight` locales (300-500 lm, rango 6-10 m, **sin sombras**) | Iluminación puntual cálida con un costo acotado |
| Entrar en cuevas o estructuras oscurece el ambiente | `InteriorLightingTrigger` planeado que ajusta `GlobalAmbientLight` a `AMBIENT_NIGHT` (`brightness = 40.0`) | Sensación de interior penumbroso sin requerir baked lightmaps |
| El sol y la luna no cambian de tamaño ni flotan al caminar | Órbita centrada en la **cámara** (`DISC_ORBIT_RADIUS = 420m`) | Un astro en el infinito no debe tener paralaje al moverse por el mapa |
| El mediodía no se alenta en GPU | Corte por iluminancia (`SHADOW_CASTING_LUX = 1.0`) | Evita que la luna renderice 4 cascadas invisibles a pleno sol (y el sol a medianoche) |
| Las sombras lejanas no gastan resolución | `maximum_distance` ajustado al alcance real de los árboles | Concentra los texeles de sombra en la geometría cercana visible |
| El horizonte se funde sin corte plano | `DistanceFog` lineal (45 m → 240 m) sincronizada con `atmosphere_color(hours)` | Si el color de niebla coincide con el cielo a esa hora, el borde desaparece |

---

## Las seis leyes de este sistema

### 1. Las sombras se cortan por iluminancia, no solo por el reloj

Bevy por defecto evalúa `shadow_maps_enabled` sin mirar la iluminancia de la luz.
Sin protección, la luna renderiza 4 pases de cascadas en pleno mediodía (y el sol a medianoche).

**Regla (§src/world/day_night.rs):** Si `illuminance < SHADOW_CASTING_LUX (1.0 lux)`, la luz **apaga sus sombras** en GPU (`shadow_maps_enabled = false`).

### 2. El número de cascadas se fija al arranque

Cambiar el número de cascadas en caliente desincroniza la contabilidad interna de Bevy
(`check_dir_light_mesh_visibility` dimensiona sus colas según los frusta) y produce
un **panic por índice fuera de rango**.

**Regla:** El conteo de cascadas (1 a 4) se fija al lanzar mediante la variable de entorno
`BOF_CASCADES` o el perfil de rendimiento. La *distancia* de sombra sí es un dial en vivo en `apply_cascade_config`.

### 3. Cero sombras en fuentes de luz puntuales (`PointLight` / `SpotLight`)

Las luces puntuales (antorchas, fogatas, cristales mágicos) proyectan luz sobre
geometrías cercanas, pero **tienen prohibido proyectar sombras dinámicas**
(`shadow_maps_enabled: false` en Bevy 0.19).

**Regla:** Solo las luces direccionales del Sol y la Luna tienen permiso de proyectar
sombras. Una luz puntual con sombras necesita hasta seis caras de cubemap; ese
costo queda fuera del baseline móvil salvo que una medición futura justifique
una excepción.

### 4. Astro en el infinito = órbita centrada en la cámara

Centrar la órbita del sol/luna en el origen del mundo $(0,0,0)$ genera un paralaje espantoso:
cruzar 112 m en un mapa de 320 m desplaza el disco solar ~14° y lo acerca de 420 m a ~308 m.

**Regla:** Los discos `SunDisc` y `MoonDisc` posicionan su transform en `camera_translation + dir * 420.0`.

### 5. Iluminación estilizada sobre PBR mate, sin toon shader

`NORTE.md` lo declara: no se usan toon shaders ni outlines fullscreen baseline. El cel-shading se reemplaza por:
- Materiales mate planos (`StandardMaterial` con `roughness ≥ 0.8`, `metallic = 0.0`).
- Sombras nítidas de 2 a 4 cascadas.
- Gradientes de atmósfera explícitos por hora.

### 6. Iluminación ambiente acotada

La luz ambiente se mantiene baja a propósito (`AMBIENT_DAY = 90.0`, `AMBIENT_NIGHT = 40.0`).
Demasiada luz ambiente elimina el contraste de las caras en sombra y aplana el volumen de mallas y terreno.

---

## Parámetros del sistema objetivo

### 1. Parámetros del Sol y la Luna (`src/world/day_night.rs`)

| Parámetro | Sol (Día) | Luna (Noche) |
|---|---|---|
| Iluminancia máxima | `10 000.0 lux` (mediodía) | `400.0 lux` |
| Color zenith / medianoche | `Color::srgb(1.0, 0.98, 0.92)` | `Color::srgb(0.55, 0.65, 0.9)` |
| Color Dawn (04:30 - 07:45) | `Color::srgb(1.0, 0.68, 0.42)` (coral) | N/A |
| Color Dusk (16:15 - 20:00) | `Color::srgb(1.0, 0.38, 0.2)` (magenta/cálido) | N/A |
| Inclinación del arco | `SUN_ARC_TILT = 0.35` (evita sombras colapsadas en línea) | Opuesto al sol (`-to_sun`) |
| Duración del día | **24.0 minutos reales** por día de juego (ritmo BOTW) | — |

### 2. Luz Ambiente y Cielo

| Parámetro | Día | Aurora (Dawn) | Crepúsculo (Dusk) | Noche |
|---|---|---|---|---|
| `GlobalAmbientLight::brightness` | `90.0` | Interpola | Interpola | `40.0` |
| Color ambiente | `srgb(1.0, 1.0, 1.0)` | `srgb(1.0, 0.65, 0.52)` | `srgb(0.9, 0.42, 0.52)` | `srgb(0.38, 0.48, 0.78)` |
| Color de cielo | `srgb(0.45, 0.68, 0.95)` | `srgb(0.95, 0.42, 0.38)` | `srgb(0.72, 0.2, 0.42)` | `srgb(0.055, 0.075, 0.17)` |

### 3. Luces Puntuales y Antorchas (`PointLight`)

`PointLight::intensity` expresa **flujo luminoso en lúmenes**, no iluminancia en
lux. Estos valores son decisiones iniciales de autoría y se afinan jugando.

| Tipo | Intensidad | Rango | Color | Sombras |
|---|---|---|---|---|
| Fogata (`Campfire`) | `500.0 lm` | `10.0 m` | `Color::srgb(1.0, 0.55, 0.2)` | `false` (Ley 3) |
| Antorcha (`Torch`) | `300.0 lm` | `6.0 m` | `Color::srgb(1.0, 0.6, 0.25)` | `false` (Ley 3) |
| Cristal Mágico (`Crystal`) | `200.0 lm` | `4.0 m` | `Color::srgb(0.2, 0.7, 1.0)` | `false` (Ley 3) |

- **Límite de Luces Activas:** Máximo 8 `PointLight` activas simultáneamente dentro del volumen de visión del jugador.

### 4. Sombras y Cascadas (`src/world/day_night.rs` & `src/perf/`)

- **Resolución Mapa de Sombras:** Configurable por perilla. Default desktop:
  `1024x1024`; perfil móvil: `512x512`; pasos disponibles:
  `2048`, `1024`, `512`.
- **Cascadas:** 4 cascadas por defecto (o 2 en perfil móvil / `BOF_CASCADES=2`).
- **Distancia de Sombras:** Ajustable dinámicamente
  (`perf.shadow_distance()`). Default desktop: `65 m`; perfil móvil: `40 m`;
  pasos disponibles: `65`, `100`, `140`, `200`, `40 m`.

### 5. Niebla Atmosférica (`src/camera/mod.rs`)

- **Tipo:** `DistanceFog` con `FogFalloff::Linear { start: 45.0, end: 240.0 }`.
- **Sincronización:** El color de la niebla lee en vivo `atmosphere_color(hours)`, asegurando continuidad con el cielo a cualquier hora del día.

---

## Orden de implementación

### Fase 1 — Ciclo Base y Sombras

- Transición continua Sol/Luna con iluminancia y colores por hora.
- Niebla lineal adaptada al color del cielo.
- Sombras cascadas de sol/luna con descarte por lux
  (`SHADOW_CASTING_LUX`).

### Fase 2 — Luces Puntuales y Transición a Interiores

- Implementar componentes `PointLight` para fogatas y antorchas con
  `shadow_maps_enabled: false` (Ley 3).
- Implementar volúmenes de colisión `InteriorLightingTrigger` que atenúan la
  luz ambiente (`GlobalAmbientLight` $\to$ `AMBIENT_NIGHT`) al entrar a cuevas.

### Fase 3 — Clima y Tormentas

- Oscurecimiento de la luz del sol/luna durante la lluvia.
- Destello instantáneo de relámpago en luz direccional
  (`50 000 lux`, 1 frame).

---

## Interfaz de diagnóstico objetivo

- F1 abre el hub; desde sus acciones se emiten
  `TimeOfDayRequest::{AdvanceHour, ToggleSpeed}`.
- F9 puede quedar como atajo de `ToggleSpeed` porque está libre.
  `[`/`]` no se usan para tiempo: ya pertenecen al editor y al navegador de
  animaciones.
- `BOF_CASCADES=N` fija el conteo al lanzar
  (ej. `BOF_CASCADES=2 cargo run`).

---

## Fuera de Alcance

Volumetric fog / God rays de alto costo (diferidos hasta contar con presupuesto GPU holgado en móvil),
Screen-space Ambient Occlusion (SSAO) no medido, y cubemaps de cielo estáticos (descartados en favor del gradiente procedural por hora).
