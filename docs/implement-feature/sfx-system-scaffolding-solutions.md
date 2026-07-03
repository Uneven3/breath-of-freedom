# Solutions: SFX System Scaffolding

This document outlines three distinct solutions for implementing the SFX system scaffolding, specifically addressing `CueMessage` dispatch and continuous SFX modulation (`BodyVelocity`/`Stamina` of `Actor` entities) under Bevy 0.19. It has been updated to address the reviewer's critiques.

---

## Solution 1: Stateful Component-Based Thresholding (Recommended)

### Approach
Define `CueId`, `CueKind`, and `CueMessage` in a shared `src/presentation/cues.rs` module, deriving `Message` for Bevy 0.19. A dedicated `SfxPlugin` is created under `src/sfx/mod.rs` registering systems in the `Update` schedule. For continuous parameter modulation, a presentation-only component `ContinuousSfxTracker` is defined in a separate data-only file `src/sfx/components.rs` (strictly adhering to **§19**). This component is used to track the last logged values of `BodyVelocity` and `Stamina` per entity. The `modulate_continuous_sfx` system dynamically queries all `Actor` entities (without hardcoding `Player` queries) for `(&BodyVelocity, &Stamina, Option<&mut ContinuousSfxTracker>)`, only logging updates when values cross a specified threshold delta (e.g., speed change > 0.5 m/s or stamina change > 5%).

### CONSTITUTION clauses at risk
* **§20 — Simulación separada de presentación:** Presentation systems running in `Update` must only read simulation components and never write to them. We mitigate this by keeping `ContinuousSfxTracker` strictly presentation-only and querying simulation components (`BodyVelocity`, `Stamina`) read-only.
* **§18 — Sin allocations en el hot path:** We must not allocate on the hot path. Component-based tracking utilizes Bevy's internal table storage which avoids heap allocations.
* **§19 — Datos separados de la implementación:** We separate the message/enum definitions (`src/presentation/cues.rs`) and the tracker component definition (`src/sfx/components.rs`) from the logic implementation (`src/sfx/mod.rs`).

### Tradeoffs
* **Pros:**
  * Clean, precise, entity-level tracking that easily scales to multi-actor environments (e.g., enemies, multiplayer) by dynamically querying `Actor` entities instead of hardcoding `Player`.
  * Safe from heap allocations in the hot path.
  * Resilient to frame rate fluctuations because it compares absolute value deltas rather than time intervals.
* **Cons:**
  * Requires adding/inserting a presentation component (`ContinuousSfxTracker`) onto simulation/actor entities.

### Edge cases
* **Initial Spawn:** When an entity is first queried, the tracker is missing. The system inserts `ContinuousSfxTracker` initialized with the current `BodyVelocity` and `Stamina` values and performs exactly one initial log to establish baseline state, avoiding duplicate logs on subsequent updates until a threshold is crossed.
* **Rapid Boundary Oscillation:** If stamina or velocity oscillates precisely around the threshold value, it could still produce frequent logs. This is mitigated by updating the tracker's stored value only when a log is actually emitted.
* **Entity Despawn:** Standard Bevy cleanup handles entity despawning automatically; the tracker is removed when the entity is destroyed, preventing leaks.

---

## Solution 2: Stateless Systems with Local State Tracking

### Approach
Implement `CueMessage` and `SfxPlugin` in the same modular structure, separating the components (like a local wrapper struct) to `src/sfx/components.rs`. In `modulate_continuous_sfx`, avoid mutating the actor entities with new components. Instead, use a system `Local<HashMap<Entity, (Vec3, f32)>>` to keep track of the last logged velocity and stamina values for each entity dynamically, emitting log statements when the current values deviate from the cached state.

### CONSTITUTION clauses at risk
* **§18 — Sin allocations en el hot path:** Using a `HashMap` in a system `Local` can cause heap allocations (rehashing, dynamic resizing, node insertions) in the hot path of `Update` as entities are added. This is a severe risk under §18.
* **§20 — Simulación separada de presentación:** Decoupled from simulation entities since no components are added to the actor entities.
* **§19 — Datos separados de la implementación:** Kept clean by moving tracking structures to a separate data file.

### Tradeoffs
* **Pros:**
  * Decouples presentation state completely from the entity component list, leaving actors clean.
* **Cons:**
  * Violates the no-allocations-in-hot-path rule (§18) during hash insertions and capacity resizing.
  * Requires manual cleanup of the local `HashMap` when entities are despawned to prevent memory leaks.

### Edge cases
* **Entity Despawning:** If the system does not explicitly prune the `Local` map, references to despawned entities will accumulate, causing a memory leak.
* **Initial Spawn Log:** Similar to Solution 1, must detect the absence of the key in the local map, insert it with baseline values, and perform a single initial log.

---

## Solution 3: Global Time-Debounced Logging

### Approach
Implement `CueMessage` and `SfxPlugin` similarly. In `modulate_continuous_sfx`, use a timer-based check (e.g., via a `Local<f32>` accumulator or `Time` resource) to limit logging of all queried `Actor` entities (dynamically retrieved) to a fixed time interval (e.g., every 500ms). When the timer triggers, iterate over all `Actor` entities, log their current `BodyVelocity` and `Stamina` values directly, and reset the timer.

### CONSTITUTION clauses at risk
* **§18 — Sin allocations en el hot path:** Extremely safe; no heap allocations are performed since we only increment a float timer.
* **§20 — Simulación separada de presentación:** Only reads actor components without modifying them.
* **§19 — Datos separados de la implementación:** Keeps all configuration data and timers in `src/sfx/components.rs` or resource structs.

### Tradeoffs
* **Pros:**
  * Extremely simple to implement and understand.
  * Guaranteed to never flood the console logs, regardless of how erratically velocity or stamina fluctuates.
  * Zero memory tracking or cleanup overhead per entity.
* **Cons:**
  * Lacks immediacy: does not log immediately when a sudden change occurs (e.g., sudden deceleration on landing or jump launch), only at the next tick of the timer.

### Edge cases
* **Varying Frame Rates:** If the frame rate drops significantly, the timer still accumulates real time correctly, but logs might appear slightly stuttered.
* **Initial Spawn:** Cannot easily isolate a single initial log per actor since logging is global and time-sliced.

---

## Chosen Solution

* **Name:** Solution 1: Stateful Component-Based Thresholding
* **Rationale:** This solution provides precise, entity-level tracking of continuous SFX modulation parameters (velocity and stamina) without causing any heap allocations on the hot path (satisfying §18). By defining the tracking state components in `src/sfx/components.rs` and dynamically querying `Actor` entities, it adheres strictly to §19 (data/implementation separation) and scales seamlessly to support multi-actor scenarios (satisfying §20). It also handles the initial spawn edge-case gracefully.

