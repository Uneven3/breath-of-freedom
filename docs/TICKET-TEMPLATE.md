# Ticket Template

Copiar este archivo a `docs/tickets/<slug>.md` por cada ticket. El agente
que lo tome (Claude, Codex, Antigravity, DeepSeek) **no comparte memoria de
sesión con quien lo escribió** — este archivo es la única fuente de verdad,
tiene que alcanzar por sí solo.

---

## Ticket: `<slug-kebab-case>`

## Sistema(s)

Uno principal. Si el ticket toca más de uno, nombrarlos todos y justificar
por qué no se puede partir en tickets separados.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — fila del/los sistema(s) de este ticket.
3. `docs/COUPLING-MAP.md` — si el ticket toca más de un sistema.
4. `docs/architecture/<sistema>.md`.
5. `docs/architecture/rationale/<los que el doc de arriba cite>.md`.
6. `docs/gdd.md` § — solo la sección citada por el doc de sistema.

No leer nada fuera de esta lista para decidir diseño — si hace falta más
contexto, es señal de que el doc de sistema está incompleto y hay que
corregirlo primero, no de que falta memoria de conversación.

## Acoplamiento (completar antes de asignar el ticket)

- Nivel según `COUPLING-MAP.md` con cada sistema vecino tocado: Tight /
  Middle / Loose / Abierto (`?`).
- **Si hay un par Tight:** ¿el otro sistema ya existe en código? Si no,
  este ticket debe esperar (secuenciar después) o construir ambos lados en
  el mismo ticket — nunca asumir la forma del otro sin haberla fijado por
  escrito primero.
- **Si hay un par Middle:** confirmar que el contrato (forma del dato que
  se lee) ya está fijado en el doc del sistema dueño. Si no, fijarlo ahí
  primero (aunque sea en un ticket previo de 10 minutos).
- **Si hay un par Abierto (`?`):** este ticket no puede escribir código que
  asuma una forma para esa relación. Si la necesita, resolverla como parte
  del ticket: decidir, documentar (actualizar el doc de sistema +
  `ARCHITECTURE-MAP.md` + `COUPLING-MAP.md`), y recién después programar.

## Alcance (File Touches)

Lista explícita de archivos/carpetas que este ticket puede crear o
modificar. Nada fuera de esta lista sin volver a esta sección primero.

- `src/<sistema>/...`
- `docs/architecture/<sistema>.md` (si el ticket cambia el diseño respecto a lo documentado)
- `docs/tickets/<slug>.md` (este archivo, para actualizar checklist y estado del desarrollo)

## Fuera de alcance

Explícito, para que el agente no "aproveche" para tocar algo cercano. Ej.:
"no migra `Single<Player>` a `Query<Actor>` — eso es `multi-actor-migration`,
otro ticket."

## Definición de terminado

- [ ] `cargo fmt` limpio.
- [ ] `cargo clippy` sin warnings nuevos (Constitución §13) — ningún
      `#[allow(...)]` sin justificación explícita en el commit.
- [ ] `cargo check`/`cargo test` pasa.
- [ ] El comportamiento coincide con `docs/architecture/<sistema>.md`. Si
      el código terminó divergiendo del diseño, el doc se actualiza en
      **este mismo ticket**, no en uno futuro.
- [ ] Invariantes de arquitectura/ECS testeadas desde ya (Constitución
      §11, excepción obligatoria): no-bleed multi-actor, ordering de
      schedule, overflow de buffers de capacidad fija, no-alloc en hot
      path, manejo de estado vacío. No esperar el checkpoint de *feeling*
      para estos.
- [ ] Tests de *feeling* jugable: solo si ya hubo checkpoint jugado — si
      no, no se agregan todavía (Constitución §10/§11).
- [ ] Sin `unsafe` (Constitución §12); sin `unwrap()`/`expect()` fuera de
      un bug de programador genuino (Constitución §8/§9).
- [ ] Si el ticket agregó/cambió una relación entre sistemas, se reflejó
      en `ARCHITECTURE-MAP.md` **y** en `COUPLING-MAP.md` — no solo en el
      doc del sistema que la originó (bug recurrente ya visto entre
      agentes: un doc anuncia la relación, el mapa no se entera).

## Notas para el agente que lo toma

*(Opcional: contexto no obvio que no cabe en los docs de arquitectura pero
ayuda — ej. "el usuario prefiere X sobre Y por esta razón de feeling".)*

---

## Ejemplo relleno (para copiar el formato, no el contenido)

## Ticket: `multi-actor-migration`

## Sistema(s)

Movement (refactor de `Single<Player>` → `Query<Actor>`).

## Lectura obligatoria

1. `docs/CONSTITUTION.md`
2. `docs/ARCHITECTURE-MAP.md` (fila `Movement`)
3. `docs/COUPLING-MAP.md` (Movement es Tight con Enemies y Multiplayer)
4. `docs/architecture/movement.md`
5. `docs/architecture/rationale/multi-actor-dispatch.md`

## Acoplamiento

Tight con Enemies y Multiplayer — ambos están **bloqueados** hasta que
este ticket termine (`BLOCKING-PREREQUISITE` en `ARCHITECTURE-MAP.md`). No
abrir worktrees de Enemies/Multiplayer en paralelo con este.

## Alcance (File Touches)

- `src/movement/mod.rs`, `src/movement/brain.rs`, `src/movement/motors/*.rs`
- `src/camera.rs` (debe seguir al actor local mediante el contrato multi-actor)
- `src/movement/spike.rs` (referencia de patrón de integración esperado)

## Fuera de alcance

No agrega Enemies ni Multiplayer — solo deja el pipeline listo para que
esos tickets puedan empezar después.

## Definición de terminado

- [ ] Los 13 motores corren sobre `Query<.., With<Actor>>` con guard
      interno por entidad, no `run_if` global.
- [ ] `src/movement/spike.rs` sigue pasando (era la prueba del patrón).
- [ ] Test nuevo: dos actores simulados en el mismo `World` no comparten
      `LocomotionState` ni se bloquean entre sí (invariante de arquitectura,
      no espera checkpoint).
