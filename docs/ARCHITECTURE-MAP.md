# Architecture Map

Vista objetivo de relaciones entre sistemas. El detalle de cada uno (datos,
estados, sistemas, decisiones abiertas) vive en `docs/architecture/<sistema>.md`. Este
documento existe para responder una pregunta operativa: **qué se puede
trabajar en paralelo, en worktrees separados, sin pisarse.**

## Categorías de relación

| Categoría | Significado | Riesgo de paralelizar sin coordinar |
|---|---|---|
| `READ` | Un sistema hace una query read-only sobre un componente/resource de otro | Bajo — el lector se adapta si el otro cambia de forma, no al revés |
| `MESSAGE` | Comunicación cruzada diferida vía `Message` de Bevy 0.19 (`MessageReader`/`MessageWriter`) | Bajo — cada lado solo necesita conocer la forma del mensaje |
| `OBSERVER-EVENT` | Reacción inmediata vía `Event`/observer de Bevy 0.19 | Medio — usar solo si la inmediatez importa más que el desacople de schedule |
| `WRITE-OWN` | Un sistema escribe sus propios componentes a partir de datos leídos de otro sistema | Bajo/medio — requiere ordenar el schedule, pero no invierte ownership |
| `SHARED-CONTRACT` | Dos sistemas comparten la *forma* de un dato (mismo `Intents`/`LocomotionState`), con un Brain distinto llenándolo | Medio — un cambio de forma (no de tuning) rompe a ambos lados; requiere declaración de `needs-contract-change` |
| `BLOCKING-PREREQUISITE` | El sistema B no tiene sentido construir hasta que A exista | Alto si se ignora — B debe esperar o mockear A explícitamente |

## Module Inventory

Cobertura cruzada contra GDD §11 (orden de prioridad de mecánicas) y §7-10
(detalle mecánico). Los sistemas sin documento propio están marcados
explícitamente — el objetivo es que este inventario nunca oculte un hueco.

| Sistema | Doc | Responsabilidad primaria | Datos clave |
|---|---|---|---|
| `Movement` | `movement.md` | Traversal del cuerpo cinemático: caminar, trepar, planear, etc. | `Intents`, `LocomotionState`, `ProposalBuffer` |
| `Combat` | `combat.md` | Ataque, parry, apuntado, stagger | `CombatIntents`, `CombatState` |
| `World` | `world.md` | Terreno, clima, ciclo día/noche y biomas | `TimeOfDay`, `Weather` |
| `Enemies` | `enemies.md` | IA hostil que controla actores no-jugador | `EnemyAiState`, `Perception` |
| `Mounts` | `mounts.md` | Locomoción de criaturas montables | `MountIntents`, `MountLocomotionState` |
| `Multiplayer` | `multiplayer.md` | Replicación host-autoritativa | `NetworkRole`, `RemoteActor` |
| `UI` | `ui.md` | HUD y prompts, solo lectura | — (sin datos propios) |
| `SFX` | `sfx.md` | Audio real + placeholder de debug | `CueMessage` (compartido) |
| `VFX` | `vfx.md` | Partículas + placeholder | `CueMessage` (compartido) |
| `Camera` | `camera.md` | Cámara de presentación: rig orbital, mouse-look/keyboard-look, landing feedback, modo apuntado y lock-on | `CameraRig`, `MouseCaptured` |
| `Health`/`Damage` | `health.md` | Vida y aplicación de daño para cualquier actor (Player, Enemies) | `Health`, `DamageRequestMessage`, `DamageAppliedMessage`, `DeathMessage` |
| `Projectiles` | `projectiles.md` | Flechas del arco: vuelo, colisión, daño | `Projectile`, `SpawnProjectileMessage` |
| `Inventory`/`Equipment` | `inventory.md` | Qué posee y qué tiene equipado cada actor; dónde vive la durabilidad de armas (GDD §7-8) | `Inventory`, `EquipmentSlots`, `Durability` |
| `Crafting` | `crafting.md` | Árbol de crafteo de equipo a partir de materiales (GDD §8, §11-5) — depende de Inventory | `Recipe`, `RecipeBook` |
| `Swim`/`Dive` | `swim.md` | Nado y buceo profundo con oxígeno/corrientes (GDD §9) — motores nuevos del plugin de Movement | `WaterFacts`, `Oxygen` |
| `Snowboard` | `snowboard.md` | Deslizar en nieve/pendientes (GDD §9) — motor nuevo del plugin de Movement | `SlopeFacts` |
| `StatusEffects` | `status-effects.md` | Frío/calor/mojado/rayo-atrae-metal (GDD §10) — cruza World↔Movement↔Combate↔Inventory | `StatusEffects`, `StatusEffect` |
| `NPCs`/`Quests` | `npcs.md` | Personajes con problemas propios, estilo Majora's Mask (GDD §11 prioridad #7) — distinto de Enemies (no hostil) | `QuestBook`, `QuestState` |
| `Persistence` | `persistence.md` | Guardado/carga, tamaño del mundo (GDD §13, decisión de producto abierta) — orquesta, no posee datos ajenos | `PersistenceSet`, `SaveSlot` |
| `Input` | `input.md` | Motor de keybinding genérico y rebindeable: resuelve input físico contra una tabla configurable y expone acciones ya resueltas por fuente, ningún Brain de gameplay lee hardware directo | `InputScheme`, `IntentAction`, `Keybindings`, `InputControlledBy`, `ActiveActions`, `ActionFrame`, `InputConsumeCursor` |
| `Debug` | `debug.md` | Logs estructurados (`tracing`/`bevy_log`, ya incluido) + overlay en vivo dentro del juego; presentación pura salvo comandos de debug explícitamente marcados | `DebugOverlay`, `DebugInspectTarget` |

## Cross-System Dependencies

| De | A | Categoría | Detalle |
|---|---|---|---|
| Combate | Movement | `READ` + `MESSAGE` | Lee `LocomotionState` (bonus sigilo); pide restricciones/interrupciones locomotoras vía `LocomotionConstraintMessage`; Movement decide el estado físico válido |
| Enemies | Movement, Combate | `SHARED-CONTRACT` | Mismo `Intents`/`CombatIntents`, Brain de IA en vez de hardware |
| Enemies | World | `READ` | Lee `TimeOfDay` para spawn/comportamiento |
| Enemies | *(fundacional)* | `BLOCKING-PREREQUISITE` | **Resuelto** — Movement ya opera sobre `Query<Actor>` (ticket `multi-actor-migration`); Enemies puede empezar (`rationale/multi-actor-dispatch.md`) |
| Mounts | Movement | `READ` + `WRITE-OWN` | Mounts lee `movement::Intents` del jinete y escribe `MountIntents` en la montura mediante `translate_mount_intents`; Movement no conoce Mounts |
| Movement, Combat, Mounts | *(núcleo compartido `src/proposal.rs`)* | `SHARED-CONTRACT` | Cada uno usa un type alias sobre `proposal::ProposalBuffer<S, N>` de capacidad fija — mismo algoritmo de arbitración, tipos concretos propios por sistema (`rationale/proposal-arbitration-core.md`) |
| Multiplayer | Movement, Combate, Mounts | `SHARED-CONTRACT` | Un jugador remoto es un `Actor` más, controlado por un `InputSource` de red; corren los mismos Brains genéricos |
| Multiplayer | *(fundacional)* | `BLOCKING-PREREQUISITE` | **Resuelto** — mismo contrato multi-actor que Enemies, ya implementado |
| Multiplayer | World | `SHARED-CONTRACT` | `TimeOfDay`/`Weather` son estado de sesión; solo el host los simula |
| UI | Movement, Combate, Mounts, Health | `READ` | Nunca escribe hacia atrás (Constitución §20) |
| SFX, VFX | Movement, Combate, World | `MESSAGE` + `READ` | `CueMessage` para sucesos discretos emitidos por simulación o colas de transición; lectura read-only en `Update` para parámetros continuos (`rationale/presentation-cues.md`) |
| Combate, Projectiles | Health | `MESSAGE` | Emiten `DamageRequestMessage`; Health valida/aplica y emite `DamageAppliedMessage`/`DeathMessage` (`rationale/health-ownership-boundary.md`) |
| Combate | Health | `MESSAGE` | Combate escucha `DamageAppliedMessage`/`DeathMessage` sobre su propio actor para decidir `CombatState::Staggered` |
| Enemies | Health | `READ` | Lee su propio `Health` para decidir `Flee` (GDD §7) |
| Combate | Projectiles | `MESSAGE` | Emite `SpawnProjectileMessage`; Projectiles posee la entidad desde el spawn |
| Projectiles | *(fundacional)* | `BLOCKING-PREREQUISITE` | Requiere `Health` para poder emitir `DamageRequestMessage` al impactar |
| World | *(nadie)* | — | Sustrato — no lee ni escribe a otros sistemas |
| Camera | Movement | `READ` | Sigue `Transform`/`LocomotionState` del actor local; hoja del grafo, nunca emisor (`camera.md`) |
| Camera | Combate | `READ` (decisión abierta) | Modo apuntado en primera persona necesita leer `CombatState::Aiming` |
| Combate | Inventory | `READ` + `MESSAGE` | Lee `EquipmentSlots` (peso/velocidad/alcance del arma activa); pide pérdida de `Durability` al conectar un golpe (`inventory.md`) |
| Crafting | Inventory | `MESSAGE` + `BLOCKING-PREREQUISITE` | Pide una transacción de inventario; Inventory valida y muta sus propios datos (`crafting.md`) |
| Swim/Dive, Snowboard | Movement | *(mismo plugin)* | No son sistemas separados — motores nuevos del mismo pipeline (`rationale/traversal-extensions-in-movement.md`) |
| Swim/Dive | Health | `MESSAGE` | `Oxygen` en 0 durante `Dive` emite `health::DamageRequestMessage` (`swim.md`) |
| Swim/Dive | World | `READ` | `services::water` lee volúmenes/geometría de agua definidos por World (`swim.md`) |
| Snowboard | World | `READ` | `services::slope` lee terreno/bioma de nieve de World (`snowboard.md`) |
| Multiplayer | Projectiles | `SHARED-CONTRACT` | Solo el host simula vuelo/colisión; clientes reciben transform replicado, mismo set `AuthoritativeSimulation` (`projectiles.md`) |
| Multiplayer | NPCs/Quests | `SHARED-CONTRACT` | `NpcRoutine` corre solo en el Host, posiciones de NPCs se replican a clientes (`npcs.md`) |
| StatusEffects | World | `READ` | Lee `Weather`/`TimeOfDay` para calcular exposición (`status-effects.md`) |
| StatusEffects | Inventory | `READ` | Lee tags de material equipado para la mecánica de rayo atrae metal |
| StatusEffects | Health | `MESSAGE` | Emite `DamageRequestMessage` por frío/calor/eléctrico extremos |
| StatusEffects | SFX, VFX | `MESSAGE` | `LightningStrike` emite `CueMessage` al impactar un rayo para reproducir sonido y destello |
| StatusEffects | Movement | `READ` (decisión abierta) | `Wet` reduciría agarre al escalar; el mecanismo exacto queda abierto |
| NPCs/Quests | World | `READ` | Rutinas de NPC leen `TimeOfDay` |
| NPCs/Quests | Inventory | `READ` + `MESSAGE` | Un quest puede requerir un ítem del `Inventory` del jugador; pide consumo vía mensaje de inventario |
| UI | NPCs/Quests | `READ` + `MESSAGE` | Prompts persistentes por estado de interacción; progreso de quest por mensaje (`npcs.md`) |
| UI | Persistence | `MESSAGE` | Único caso donde UI emite: dispara `SaveRequestMessage`/`LoadRequestMessage` desde el menú (`ui.md`, `persistence.md`) |
| UI | Inventory, Crafting | `READ` | Menú de inventario/crafteo, read-only (`ui.md`) |
| UI | Input | `READ` + `MESSAGE` | Menú de rebinding: UI lee `Keybindings` y emite `RebindRequestMessage`; Input valida y escribe la tabla (`input.md`) |
| Persistence | *(todos los sistemas persistibles)* | `WRITE-OWN` (por cada dueño) | Persistence orquesta el `SystemSet` de guardado/carga; cada sistema serializa sus propios datos, Persistence no los conoce (`rationale/persistence-orchestration.md`) |
| Movement, Combate, NPCs/Quests | Input | `READ` + `WRITE-OWN` | Leen `InputControlledBy` + `ActiveActions` por `InputSource` y mutan su propio `InputConsumeCursor` para gatillos; nunca leen `ButtonInput<KeyCode>` ni escriben `Keybindings` (`input.md`, `rationale/data-driven-keybindings.md`) |
| Camera | Input | `READ` | Elige `mouse_look` vs `keyboard_look` según `InputScheme`, y lee `ActiveActions` del `InputSource` local para `LookAxis` bajo `KeyboardOnly` (`camera.md`) |
| Persistence | Input | `WRITE-OWN` (por Input dentro de `PersistenceSet`) | Persiste `Keybindings` (preferencia del jugador, no estado de partida) — mismo mecanismo que el resto de dueños (`input.md`) |
| Multiplayer | Input | `MESSAGE` + `SHARED-CONTRACT` | `LocalInputFrame` empaqueta `input::ActionFrame` ya resuelto localmente (no hardware crudo); Multiplayer emite `ApplyRemoteActionsMessage` e Input escribe su propio `ActiveActions` del `InputSource` de red — no hay `NetworkBrain` de traducción separado (`input.md`, `multiplayer.md`) |
| Debug | *(todos los sistemas)* | `READ` | Overlay/logs leen cualquier componente/resource público — mismo trato que UI/Camera; la única excepción de escritura del proyecto son comandos de debug explícitamente marcados (`debug.md`) |

## Coordinación de trabajo

- **Trabajo aditivo:** World, UI, SFX/VFX y Combat se diseñan como consumidores
  o extensiones aditivas de contratos existentes.
- **Prerequisito fundacional (Movement: resuelto):** Enemies y Multiplayer
  requieren que Movement y Combat operen sobre `Actor` genérico en vez de
  asumir un único jugador. Movement ya lo cumple (`multi-actor-migration`);
  Combat sigue pendiente de la misma migración.
- **Mounts:** usa pipeline propio y un sistema de traducción de input ordenado
  después de `MovementSet::ReadIntents` y antes de `MountSet::GatherProposals`;
  no requiere que `movement::brain` importe tipos de Mounts.
