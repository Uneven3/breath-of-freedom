# Rationale: cuándo NO usar el patrón Broker

## El problema

Movement, Combat y Mounts comparten el patrón Brain → Intents → Broker →
Motors → Body (con arbitración de propuestas). Al diseñar Health y
Projectiles apareció la pregunta: ¿estos también necesitan su propio
`Intents`/`State`/`ProposalBuffer`?

## La decisión: no, y esta es la regla para decidirlo

El patrón Broker resuelve un problema específico: **un actor con varios
comportamientos mutuamente excluyentes**, donde más de uno puede querer
activarse el mismo frame y hace falta un árbitro (por eso Movement tiene 13
motores peleando por un solo `LocomotionState`). Existe *porque* hay
conflicto real que resolver.

Health y Projectiles no tienen ese conflicto:

- **Health** es un pool que sube o baja — no hay "estados" compitiendo por
  ser el estado activo, solo un valor y un evento que lo modifica.
- **Projectiles** tiene exactamente un comportamiento (volar hasta
  impactar) — no hay un segundo motor proponiendo algo distinto para
  reemplazarlo.

Envolver esto en Intents/Broker sería una abstracción sin problema que
resolver — el mismo tipo de sobre-ingeniería que la Constitución pide evitar
(no diseñar para casos hipotéticos), aplicada al patrón arquitectónico en
vez de al código.

## Regla general para sistemas futuros

Antes de copiar el patrón Broker a un sistema nuevo, preguntar: ¿hay más de
un comportamiento que puede querer ser "el activo" el mismo frame, para el
mismo actor? Si la respuesta es no, un `Component` simple + `Message`s de
entrada/salida es suficiente y más honesto sobre la complejidad real del
sistema. En Bevy 0.19, estos contratos diferidos usan `MessageReader`/
`MessageWriter`; los `Event`/observers quedan para reacciones inmediatas
explícitas. (codex)
