# Rationale: Documentación como arquitectura objetivo

## El problema

La documentación mezclaba dos responsabilidades distintas:

- describir la arquitectura y el diseño que queremos construir;
- reportar qué parte de ese diseño ya existe en el código.

En un proyecto con múltiples agentes trabajando sin memoria compartida, esa
mezcla es peligrosa: un agente puede tomar un reporte de estado viejo como si
fuera una restricción de diseño. El estado real ya tiene una fuente mejor: el
código, los tests y el diff.

## Decisión

Los documentos de alto nivel (`AGENTS.md`, `gdd.md`,
`docs/ARCHITECTURE-MAP.md` y `docs/architecture/*.md`) deben describir la
arquitectura objetivo y las decisiones de diseño vigentes. No deben intentar
mantener un inventario de implementación actual.

La historia, alternativas rechazadas y el porqué viven en
`docs/architecture/rationale/`. Cuando un rationale rechaza una alternativa,
ningún documento operativo debe seguir presentando esa alternativa como plan
de implementación.

## Aplicación

1. **Stack:** `AGENTS.md` declara el stack objetivo soportado por el proyecto.
2. **GDD:** describe visión, pilares, mecánicas y prioridades de producto, no
   estado de avance.
3. **Architecture Map:** describe contratos y dependencias entre sistemas, no
   qué módulos existen ya.
4. **Docs de sistema:** describen datos, sistemas, relaciones y decisiones
   abiertas del diseño objetivo.
5. **Rationales:** explican por qué se tomó una decisión y qué alternativas se
   descartaron.

## Regla para futuros cambios

Si una decisión cambia, primero se actualiza o agrega el rationale que explica
el cambio. Luego se actualizan los documentos operativos para que apunten a la
decisión vigente. No debe quedar una decisión rechazada escrita como si fuera
el plan actual.

Si hace falta saber el estado de implementación, se revisa el código.

## Revisión pre-implementación

Antes de empezar a escribir código, la documentación normativa debe ser
internamente consistente: los números de reglas se ordenan de forma legible,
los mapas no describen contratos rechazados y los documentos operativos no
mezclan arquitectura objetivo con estado actual del repo. Por eso se reordenó
`docs/CONSTITUTION.md` para que §17 y §18 aparezcan antes de §19 y §20, sin
renumerar reglas ni cambiar el significado de las referencias existentes.
(codex)

También se reemplazaron referencias a conversaciones o al "mapa actual" por
enlaces a rationales y al mapa vigente. Un agente nuevo debe poder reconstruir
la decisión desde archivos del repo, no desde memoria de sesión ni desde un
snapshot informal del estado de implementación. (codex)

Los templates de tickets también deben expresar alcance objetivo. Pueden
nombrar archivos que probablemente se toquen, pero no deben depender de una
descripción del código actual para justificar el trabajo; esa verificación se
hace leyendo el código al tomar el ticket. (codex)
