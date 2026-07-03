# Architecture Constitution

> **Absolute Laws of the System.**
> Código que viole estas leyes NO debe implementarse ni mergearse.
> Escrita para Bevy/Rust/ECS. Todo agente (Claude, Codex, Antigravity,
> DeepSeek, …) que escriba código en este repo debe cumplirlas.

## Tier 1 — Principios generales (SOLID aplicado a ECS)

### §1 — Responsabilidad única
Cada plugin/sistema/componente tiene una sola razón para cambiar. Un sistema
que mezcla responsabilidades no relacionadas se separa.

### §2 — Abierto/cerrado, de forma aditiva
Se extiende comportamiento agregando sistemas/componentes/plugins nuevos, no
editando la lógica interna de un sistema no relacionado para forzar un caso
nuevo.

### §3 — Contratos de trait se cumplen sin sorpresas
Si un tipo implementa un trait, su comportamiento debe honrar lo que ese
trait promete — nada de implementaciones que rompen la expectativa del
llamador.

### §4 — Interfaces chicas
Las APIs públicas (traits, funciones públicas de un módulo) exponen solo lo
que el llamador necesita. Nada de forzar a depender de métodos que no se usan.

### §5 — Inversión de dependencias vía componentes/mensajes
Un sistema depende de los componentes/mensajes que otro sistema expone, nunca
de su implementación interna. Leer estado de otro sistema es una query o un
mensaje, no acceso directo a sus internals.

### §6 — Los datos no tienen lógica
Components, Resources, Messages y Events son datos puros. Toda la lógica vive
en sistemas (getters/helpers puros están bien).

### §7 — Los datos fluyen hacia abajo, los mensajes hacia arriba
Un sistema muta lo que posee directamente. En Bevy 0.19, la comunicación
cruzada diferida usa `Message`s (`#[derive(Message)]`, `MessageReader`,
`MessageWriter`) o lecturas read-only — nunca mutación directa de estado que
pertenece a otro sistema.

`Event`s/observers quedan reservados para reacciones inmediatas donde esa
semántica sea explícitamente necesaria; por defecto, los contratos entre
plugins usan `Message`.

## Tier 2 — Errores y tipos

### §8 — Evitar `unwrap()`/`expect()`
Se evita su uso. El objetivo es aprovechar el sistema de tipos de Rust para
que los estados inválidos sean **irrepresentables**, en vez de descubrirlos
en runtime con un panic.

### §9 — Panic es para bugs de programador, no para el juego
Un panic solo es aceptable ante una invariante rota que el tipo debería haber
garantizado (un bug). Cualquier condición que puede pasar por el juego mismo
o por datos (asset faltante, input inválido, estado de red) se modela con
`Result`/`Option` y se maneja, nunca con un panic.

## Tier 3 — Testing

### §10 — Concepto: Checkpoint
Un **checkpoint** es el punto en que un comportamiento ya fue validado
jugándolo — se siente/funciona bien — no una suposición de que está bien.

### §11 — Los tests llegan después del checkpoint
No se testea comportamiento que todavía no llegó a un checkpoint (fijaría
algo que puede cambiar). Una vez alcanzado el checkpoint, se testea de forma
extensiva para proteger ese comportamiento de regresiones futuras.

Excepción obligatoria: invariantes de arquitectura, seguridad y ECS se pueden
y deben testear desde el diseño, aunque el *feeling* jugable todavía no haya
llegado a checkpoint. Ejemplos: no bleed de estado entre actores, ordering de
schedule, overflow de buffers fijos, contratos multi-actor, ausencia de
allocations en hot paths y manejo de estados vacíos.

## Tier 4 — Seguridad y herramientas

### §12 — `unsafe` evitado por completo
Este proyecto no escribe `unsafe`. El riesgo de un bug no detectado supera
cualquier beneficio de rendimiento — no hay en el proyecto quien lo revise
con confianza. (Que una dependencia use `unsafe` internamente es su
responsabilidad, no la nuestra.)

### §13 — `clippy` y `fmt` de uso extensivo, warnings no se ocultan
`cargo fmt` y `cargo clippy` se corren antes de dar por terminada una tarea.
Un warning se soluciona, no se silencia con `#[allow(...)]` como atajo salvo
justificación explícita y puntual.

## Tier 5 — Organización y estilo

### §14 — Un plugin de Bevy por sistema
Cada sistema nuevo (combate, monturas, clima, …) vive en su propia carpeta
bajo `src/` con su propio `XPlugin`, siguiendo el patrón ya establecido en
`src/movement/`.

### §15 — Comentarios: evitar por defecto
Sin comentarios salvo que sea genuinamente necesario explicar algo que el
código no puede decir por sí mismo (una restricción no obvia, una invariante
sutil, un workaround). Nunca explicar el *qué* — el nombre ya lo dice.

### §16 — Tamaño como heurística, no como bloqueo
~300 líneas en una función/archivo/sistema es señal de considerar dividir,
no una regla dura que bloquea un merge.

## Tier 6 — Dependencias

### §17 — Crate nuevo requiere aprobación humana
Ningún agente agrega una dependencia nueva a `Cargo.toml` sin proponerla
primero y esperar el OK del mantenedor del proyecto. Evita que agentes
distintos traigan soluciones distintas al mismo problema.

## Tier 7 — Rendimiento

### §18 — Sin allocations en el hot path de `FixedUpdate`
El loop de física/gameplay a 60Hz no asigna memoria en el camino caliente,
desde etapas tempranas del proyecto.

## Tier 8 — Separación de datos/simulación

### §19 — Datos separados de la implementación
Cada sistema separa sus componentes/mensajes/enums (datos) de las funciones
que operan sobre ellos (lógica) en archivos distintos — nunca mezclados en
el mismo archivo (ej. `state.rs`/`intents.rs`/`proposal.rs` vs. `mod.rs`/
sistemas, como ya hace `src/movement/`).

### §20 — Simulación separada de presentación
La lógica de juego (reglas, física, arbitración) vive en `FixedUpdate` y
nunca depende de nada visual. Cámara, interpolación visual, HUD y
audio-cues viven en `Update` y solo *leen* el estado de la simulación —
nunca lo escriben.

> **Enforcement:** el borrow checker y el sistema de tipos de Rust exigen
> parte de esto estructuralmente; el resto es contrato social — toda
> violación se trata como bug, sin importar la intención.
