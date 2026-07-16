# Ticket: `combat-game-feel` — IMPLEMENTADO (pendiente checkpoint jugado)

## Sistema(s)

Presentation (nuevo `presentation/juice.rs`), con contratos nuevos en Combat
(`HitImpactMessage`), Movement (`BodyImpulseMessage`) y Camera
(`CameraShake`).

## Qué se construyó (pedido del usuario, 2026-07-15)

Todo alimentado por **`combat::motors::attack::HitImpactMessage`**
(`{ target, position, damage, critical }`) — Combate lo publica al resolver
un golpe y no sabe quién lo consume:

| Efecto | Dónde | Cómo |
|---|---|---|
| **Knockback** al ser golpeado | `movement/constraints.rs::BodyImpulseMessage` (Movement es dueño; Combate emite) | Empujón planar 6.5 m/s (10 en crítico) sumado a `BodyVelocity`; la fricción del motor activo lo reabsorbe — un shove, no un estado. Actor-genérico: sirve para enemigo y jugador. |
| **Flash blanco** al ser golpeado | `juice::HitFlash` sobre la entidad *visual* | Material a blanco HDR 0.12 s y restaura el color original; `tint_enemy_visual` respeta el flash (`Without<HitFlash>`). Actor-genérico vía `visuals::VisualOf` (link uniforme visual→actor, nuevo). |
| **Hit burst procedural blanco** | `juice::burst_on_hit` | 8 esferitas unlit en abanico de ángulo áureo (determinista, sin RNG), desaceleran y se encogen en 0.22 s. |
| **Texto flotante de daño** | `juice::damage_text_on_hit` | UI proyectada con `world_to_viewport`, sube y desvanece 0.8 s; crítico = más grande y dorado. Enemigo y jugador (genérico por target). |
| **Jelly al saltar/aterrizar** | `juice::Jelly` en cada visual con `VisualOf` | Squash & stretch (+0.28 al despegar, −0.24 al aterrizar), volumen semi-conservado, recuperación exponencial. Player (GLTF), probe y bokobo. |
| **Camera shake al recibir daño** | `camera::CameraShake` (trauma²) | Presentation agrega trauma cuando el target es `Player`; decae en tiempo *real* (funciona durante hitstop). Mecanismo cableado — se ejercita cuando el bokobo ataque (`enemies-combat`). |
| **Flash de pantalla al recibir daño** | `juice::ScreenFlash` (nodo UI fullscreen) | Mismo gating por `Player`; también espera a `enemies-combat`. |
| **Hitstop en crítico** ("pausa de cámara") | `juice::Hitstop` | 90 ms de `Time<Virtual>::set_relative_speed(0.0)` — `Time<Fixed>` acumula desde virtual, así que TODA la simulación pausa coherente y retoma sin drift; el countdown corre en `Time<Real>`. **No es** el slow-mo del flurry rush descartado: pausa fija, mínima, nada la lee ni pelea con ella. |

## Decisiones de arquitectura

- `HitImpactMessage` no lleva `attacker`: ningún consumidor lo lee (Health
  tendrá su propio `DamageRequestMessage` con atribución). Campo cuando
  alguien lo lea.
- `VisualOf(Entity)` unifica los tres visuales (player/probe/enemy) para
  efectos transversales; el del player se enlaza lazy (visual y cuerpo
  spawnnean en `Startup` sin orden garantizado).
- Presentation es read-only sobre simulación + dueña de sus entidades
  efímeras (partículas, textos). La única escritura "hacia atrás" es el
  hitstop sobre `Time<Virtual>`, documentada en el módulo.

## Checkpoint de feeling (pendiente — el usuario juega)

Combo al bokobo: flash + burst + número + knockback en cada golpe;
sneakstrike: pausa de 90 ms + número dorado grande + knockback fuerte;
saltar por ahí: jelly al despegar y aterrizar (también en probe/bokobo).

## Definición de terminado

- [x] fmt/clippy/test limpios (137 tests).
- [x] Invariantes §11: impulso no-bleed dirigido por entidad (test de
      constraints), jelly solo en despegue/aterrizaje (transiciones puras
      testeadas).
- [x] Docs sincronizados (`combat.md`, `movement.md`, `WORKING-CONTEXT.md`).
- [ ] Checkpoint jugado + tuning de intensidades.
