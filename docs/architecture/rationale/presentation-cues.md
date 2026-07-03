# Rationale: Un solo bus de "cues" para mensajes discretos de Audio/VFX (antigravity, codex)

## El problema

SFX y VFX necesitan reaccionar a los mismos sucesos del juego (transiciones de `LocomotionState`/`CombatState`, golpes, cambios de clima), pero son sistemas de presentación distintos. Si cada uno realiza sus propias queries de transición, la lógica se duplica y se vuelve propensa a errores.

## La decisión: CueMessage para sucesos discretos

Se define un solo tipo de mensaje compartido para Bevy 0.19, `CueMessage { id: CueId, kind: CueKind }` (`CueKind::Audio | CueKind::Vfx`), derivado con `#[derive(Message)]`. (codex)
Los sistemas de simulación emiten cues discretos en el tick donde ocurre el
suceso o escriben una cola de transiciones por actor que presentación consume
en `Update`. No se depende únicamente de `Changed<T>` en `Update`, porque
varios ticks de `FixedUpdate` pueden ocurrir antes de un frame visual y se
perderían transiciones intermedias. Los plugins de `SFX` y `VFX` escuchan
este bus de mensajes, abstrayéndose de las reglas internas de simulación que
los originaron. (codex)

---

## Modulación de Parámetros Continuos (Alineación con Constitución §20)

### La Aclaración
Originalmente se sugirió que *"SFX/VFX nunca lee un componente de simulación directamente"*. Sin embargo, esta restricción es un **supuesto erróneo** que impide la implementación de efectos continuos y dinámicos:
* **Audio del Viento:** El volumen y pitch del viento deben cambiar gradualmente con respecto a la velocidad de caída (`BodyVelocity`).
* **Respiración Agitada:** El tono de respiración debe cambiar dinámicamente según el nivel de `Stamina`.
* **Sonido de Lluvia:** El volumen debe mezclarse según la intensidad en tiempo real de `Weather`.

Forzar que estos valores continuos se transmitan por `CueMessage` inundaría el bus de mensajes cada frame, acoplando severamente el emisor con las necesidades específicas del audio/VFX. (codex)

### Regla Final
De acuerdo con `docs/CONSTITUTION.md` §20, los sistemas de presentación (Audio, VFX, HUD, Cámara) corren en `Update` y **tienen permitido realizar lecturas directas (read-only) del estado de simulación** (`BodyVelocity`, `Stamina`, `Weather`, `Transform`) para modular parámetros de loops continuos o emisores de partículas. (codex)

Se utiliza **`CueMessage`** únicamente para disparar disparadores discretos (un paso, un impacto, un destello, la activación de un planeador). (codex)
