# Rationale: Migración a Bevy 0.19 y Avian3D 0.7.0 (antigravity)

## Contexto

El proyecto migra del stack Bevy 0.18 / Avian3D 0.6.1 a Bevy 0.19 / Avian3D 0.7.0.
Esta migración introduce mejoras de rendimiento (BSN, contact shadows, culling) y actualiza varias firmas de tipos.

---

## 1. Comprobación de Fallibilidad de `Query::single` / `Query::single_mut`

Tras realizar pruebas de compilación y validar el comportamiento de Bevy 0.19.0:
* Se confirma que, desde Bevy 0.16+, **`Query::single()`** y **`Query::single_mut()`** son métodos **fallibles** que retornan `Result<T, QuerySingleError>` (reemplazando el antiguo `get_single`).
* El uso de `q.single()` y `q.single_mut()` en nuestro código (ej. en la condición de ejecución `in_loco_state` y en `reset_climb_toggle`) es correcto y seguro en Bevy 0.19, por lo que se conserva la firma original para evitar pánicos y mantener la compilación limpia.

---

## 2. Cambios de Campos y Tipos en la API de Bevy 0.19

Se aplicaron los siguientes ajustes necesarios para compilar contra Bevy 0.19:

### A. Luces Direccionales: `shadows_enabled` → `shadow_maps_enabled`
En `src/world.rs` (L86-L89):
* **Motivo:** En Bevy 0.19, el campo `shadows_enabled` de la estructura `DirectionalLight` ha sido renombrado a `shadow_maps_enabled` para reflejar con mayor precisión el uso de mapas de sombras en cascada.

### B. Tamaño de Fuente en Textos UI: `f32` → `FontSize`
En `src/debug.rs` (L29-L32):
* **Motivo:** En Bevy 0.19, el campo `font_size` de `TextFont` ya no es un `f32` plano, sino una opción estructurada del tipo enum `FontSize`. Se cambia de `16.0` a `FontSize::Px(16.0)`.
