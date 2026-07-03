# Worktree Workflow

Cómo pasar de un ticket (`docs/TICKET-TEMPLATE.md`) a un worktree que un
agente puede tomar de forma independiente, sin pisar a otro agente
trabajando en paralelo.

## Cuándo abrir un worktree

Un ticket = un worktree cuando: va a tomar más de una sesión, o se lo vas
a asignar a un agente distinto del que ya está trabajando en `main` u otro
worktree. Para cambios de una sola línea no hace falta — se trabaja
directo en `main`.

## Antes de abrir uno: chequeo de acoplamiento

Revisar `docs/COUPLING-MAP.md` contra **todos** los worktrees ya abiertos
(`git worktree list`):

| Nivel con un worktree ya abierto | Qué hacer |
|---|---|
| **Tight** | No abrir en paralelo. Esperar a que el otro termine y mergee, o coordinar el contrato explícitamente por escrito (actualizar el doc de sistema) antes de arrancar los dos a la vez. |
| **Middle** | Se puede abrir en paralelo *si* el contrato (forma del dato que se lee/escribe) ya está fijado por escrito en el doc del sistema dueño. Si no está fijado, fijarlo primero (ticket corto aparte). |
| **Loose** o sin relación | Abrir libremente. |
| **Abierto (`?`)** | No abrir un worktree que dependa de esa relación todavía — resolver la decisión abierta primero (puede ser el ticket mismo, ver plantilla). |

Esto es además de lo obvio: dos worktrees que tocan el **mismo archivo**
van a conflictuar en el merge sin importar el nivel de acoplamiento del
sistema — el "File Touches" del ticket ya debería haber anticipado esto.

## Convención de nombres

- Carpeta: `../breath-of-freedom-<slug>` (sibling del repo principal, nunca
  anidada dentro de él).
- Rama: `feature/<slug>`.

## Comandos

```bash
# Abrir
git worktree add ../breath-of-freedom-<slug> -b feature/<slug>

# Ver todos los worktrees activos (chequear acoplamiento contra estos)
git worktree list

# Cerrar (después de mergear)
git worktree remove ../breath-of-freedom-<slug>
```

## Qué lleva el worktree

El archivo de ticket (`docs/tickets/<slug>.md`, copiado de
`docs/TICKET-TEMPLATE.md`) se commitea **dentro del worktree** en el primer
commit, no queda solo en la conversación con el usuario — el agente que lo
abra no tiene memoria de esta sesión, el archivo es su única fuente de
verdad además de `docs/CONSTITUTION.md`/`docs/ARCHITECTURE-MAP.md`/
`docs/COUPLING-MAP.md`, que ya viven en el repo.

## Al cerrar un ticket

1. Dentro del worktree: `cargo fmt && cargo clippy && cargo test` limpio.
2. Si el ticket cambió el diseño real respecto a lo documentado, actualizar
   `docs/architecture/<sistema>.md` (y `ARCHITECTURE-MAP.md`/
   `COUPLING-MAP.md` si cambió una relación) **en el mismo commit** — nunca
   dejar el doc desincronizado del código, es el bug más recurrente que
   aparece entre revisiones de agentes distintos.
3. Avisar al usuario. El merge a `main` (y cualquier push) lo decide él,
   no el agente.
4. Remover el worktree después del merge: `git worktree remove ../breath-of-freedom-<slug>`.
5. Eliminar la rama de feature local ya fusionada para mantener limpio el repositorio: `git branch -d feature/<slug>`.

## Orden sugerido según el mapa vigente (ejemplo, no un backlog fijo)

Sacado de `docs/COUPLING-MAP.md` § Pares Tight — quién bloquea a quién en el
contrato de arquitectura:

1. **`multi-actor-migration`** (Movement) y **`proposal-core-extraction`**
   (`src/proposal.rs`) — primero. Bloquean a Enemies y Multiplayer
   (multi-actor) y son el contrato compartido de Combat/Mounts
   (núcleo de arbitración). Ninguno de los dos depende del otro entre sí —
   se pueden hacer en worktrees paralelos.
2. **Health, World, Input** — sin bloqueos, en paralelo entre sí y con lo
   de arriba. Son la base que varios sistemas van a leer después.
3. **Combat** (necesita el núcleo de `proposal.rs` del paso 1 y `Health`
   del paso 2) y **Inventory** (sin bloqueos) — en paralelo entre sí.
4. **Projectiles** (necesita `Health`), **Crafting** (necesita
   `Inventory`), **Mounts** (necesita el núcleo de `proposal.rs`) — los
   tres en paralelo entre sí, cada uno solo Tight con un paso anterior ya
   cerrado.
5. **Enemies** y **Multiplayer** (ambos necesitan `multi-actor-migration`
   cerrado, más Combat existiendo) — recién acá, y ojo: son Tight *entre
   sí* con Movement/Combat/Mounts, no entre ellos dos directamente, así que
   se pueden encarar en paralelo una vez que el paso 1 cerró.
6. **StatusEffects, NPCs/Quests** — tocan varios sistemas ya construidos
   (World/Inventory/Health), mejor al final de esa cadena.
7. **Persistence** — último a propósito: orquesta el guardado de todos los
   demás, necesita que sus formas ya estén razonablemente estables.

UI/SFX/VFX/Camera son aditivos y Loose con casi todo — se pueden intercalar
en cualquier punto sin esperar este orden.
