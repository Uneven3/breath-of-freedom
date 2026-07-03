# Plan: SFX System Scaffolding

This document outlines the step-by-step implementation plan for the SFX System Scaffolding feature.

## File Touches

* `src/presentation/mod.rs` (new)
* `src/presentation/cues.rs` (new)
* `src/sfx/components.rs` (new)
* `src/sfx/mod.rs` (new)
* `src/main.rs` (modify)
* `src/movement/mod.rs` (modify)

## Pre-implementation Checklist

- [x] All code conforms to `docs/CONSTITUTION.md` and the architecture maps.
- [x] No `unwrap()` or `expect()` are introduced.
- [x] The `Actor` query is implemented generically without hardcoding a query filter for `Player`.
- [x] Heap allocations are avoided in the hot path of `Update`.
- [x] Data/implementation separation rules (§19) are strictly followed.

## Core Logic Flow

1. **Expose Presentation and SFX Modules:**
   Create `src/presentation/mod.rs` and `src/sfx/mod.rs` as module entry points. Declare `mod presentation;` and `mod sfx;` in `src/main.rs`.
   
2. **Define Discrete Cues and Messages:**
   In `src/presentation/cues.rs`, define:
   - `CueId` (enum with variants: `Step`, `Jump`, etc.) deriving `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`.
   - `CueKind` (enum with variants: `Audio`, `Vfx`) deriving `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`.
   - `CueMessage` (struct with fields `id: CueId` and `kind: CueKind`) deriving `Message`, `Clone`, `Debug`.
   Expose these types in `src/presentation/mod.rs`.

3. **Register CueMessage on App:**
   In `src/main.rs`, register the `CueMessage` type on the Bevy App using `.add_message::<presentation::cues::CueMessage>()`.

4. **Define Presentation components (Adhering to §19):**
   In `src/sfx/components.rs`, define `ContinuousSfxTracker` as a Bevy component containing tracking states:
   - `last_speed: f32`
   - `last_stamina: f32`
   Expose this component in `src/sfx/mod.rs` or via a sub-module.

5. **Define Actor Component:**
   In `src/movement/mod.rs`, define a public `Actor` marker component: `pub struct Actor;`. Add `Actor` component to the entity spawned in `spawn_player` in `src/movement/mod.rs`.

6. **Implement SfxPlugin:**
   In `src/sfx/mod.rs`, implement `SfxPlugin` which registers the systems `log_audio_cue` and `modulate_continuous_sfx` in the `Update` schedule. Register `SfxPlugin` in `src/main.rs`.

7. **Implement log_audio_cue System:**
   In `src/sfx/mod.rs`, implement `log_audio_cue` running in `Update`. It reads `CueMessage` via `MessageReader<CueMessage>` and, if `kind == CueKind::Audio`, prints a debug log statement using `log::debug!` (or `bevy::log::debug!`).

8. **Implement modulate_continuous_sfx System:**
   In `src/sfx/mod.rs`, implement `modulate_continuous_sfx` running in `Update`. Query all `Actor` entities (using `Query<(Entity, &BodyVelocity, &Stamina, Option<&mut ContinuousSfxTracker>), With<Actor>>`).

9. **Apply Debouncing/Thresholding Logic with Initial Log:**
   For each queried actor in `modulate_continuous_sfx`:
   - Calculate current speed as the magnitude of `BodyVelocity`.
   - If `ContinuousSfxTracker` is absent:
     - Perform exactly one initial log (e.g. `"Initial continuous SFX baseline: speed={:.2}, stamina={:.2}"`) using `log::debug!`.
     - Insert `ContinuousSfxTracker` initialized with the current speed and stamina values via `commands.entity(entity).insert(ContinuousSfxTracker { last_speed, last_stamina: stamina.current })`.
   - If `ContinuousSfxTracker` is present:
     - Check if `(current_speed - tracker.last_speed).abs() > 0.5` OR `(stamina.current - tracker.last_stamina).abs() > 0.05`.
     - If true, emit a debug log statement detailing the updated values, and update `tracker.last_speed = current_speed` and `tracker.last_stamina = stamina.current`.

10. **Syntax and Lint Check:**
    Verify compilation via `cargo check` and run `cargo clippy` to ensure no warnings or lint errors.

### Fidelity Check

| Step | Location | Notes |
| :--- | :--- | :--- |
| Step 1 | [src/main.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/main.rs#L11-L15) | Declared `presentation` and `sfx` modules. |
| Step 2 | [src/presentation/cues.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/presentation/cues.rs#L1-L25) | Defined `CueId`, `CueKind`, and `CueMessage` deriving `Message`. |
| Step 3 | [src/main.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/main.rs#L27) | Registered `CueMessage` message type on Bevy app. |
| Step 4 | [src/sfx/components.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/sfx/components.rs#L1-L8) | Defined presentation component `ContinuousSfxTracker` for tracking continuous SFX state. |
| Step 5 | [src/movement/mod.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/movement/mod.rs#L41-L43) | Defined `Actor` marker component and attached it to Player entity in `spawn_player`. |
| Step 6 | [src/sfx/mod.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/sfx/mod.rs#L13-L20) | Implemented `SfxPlugin` registering continuous and discrete systems. |
| Step 7 | [src/sfx/mod.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/sfx/mod.rs#L22-L29) | Implemented `log_audio_cue` filtering for `CueKind::Audio` and logging via `debug!`. |
| Step 8 | [src/sfx/mod.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/sfx/mod.rs#L33-L45) | Implemented `modulate_continuous_sfx` querying `Actor` entities dynamically. |
| Step 9 | [src/sfx/mod.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/sfx/mod.rs#L47-L70) | Applied delta-based debouncing and one-time baseline log for `ContinuousSfxTracker`. |
| Step 10 | [src/sfx/mod.rs](file:///home/francisco/Programming/uneven/breath-of-freedom-sfx-system-scaffolding/src/sfx/mod.rs#L1-L71) | Verified compilation via `cargo check` and clean `cargo clippy`. |

