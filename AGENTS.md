# AGENTS.md

**Breath of Freedom** — juego de acción-aventura de mundo abierto en Bevy
(Rust), open-source (GNU GPL), que busca el *feeling* de Breath of the Wild
sin nada de la IP de Zelda (mundo, historia, razas y assets propios).

Ver `docs/NORTE.md` para la visión completa del juego.

## Stack

- Rust + Bevy 0.19 + Avian3D 0.7.0 (física).
- `cargo check` / `cargo build` / `cargo test` desde la raíz del repo.

## Cómo se construye este proyecto

Este proyecto se desarrolla con **múltiples agentes/herramientas de IA en
paralelo** (Claude, Codex, Antigravity, DeepSeek, …), muchas veces en
worktrees separados y sesiones sin memoria compartida entre sí. Por eso:

- La documentación tiene **dos niveles**:
  - **Coordinación (4 archivos core, ≤1000 líneas totales):** son la fuente de
    verdad que todo agente lee primero.
    `docs/ARCHITECTURE.md` (leyes + arquitectura, ≤200),
    `docs/NORTE.md` (visión y roadmap, ≤200),
    `docs/AHORA.md` (trabajo presente, ≤500),
    `docs/ASSET_PIPELINE.md` (contrato Blender→Bevy, ≤250).
    Lo cerrado se borra de AHORA.md (queda en git).
  - **Referencias técnicas de dominio** (`docs/BOTWGrass.md`,
    `docs/BOTWMovements.md`, `docs/CHARACTER_ANIMATION_IK.md`,
    `docs/GraphicalTechniques.md`, `docs/TEXTURES.md`, `docs/LIGHTING.md`,
    `docs/PARTICLES.md`, `docs/AUDIO.md`):
    planes de implementación por feature, con la misma disciplina de
    honestidad y medición. Describen **lo que se quiere construir**; no son
    inventarios del código existente ni llevan el estado vivo de implementación.
    Cada uno es dueño de su tema; no duplican contenido entre sí (un tema = un
    documento). Los core los referencian, no los repiten.
  - **Planes de migración**: trabajo de ingeniería transversal, con fases y
    criterios de aceptación por fase. **Temporales por definición** — se borran
    cuando su última fase cierra, igual que AHORA.md borra lo cerrado, y lo vivo
    se muda a los core. No fijan leyes: las leyes viven en `ARCHITECTURE.md`.
    El último fue `docs/CRATES.md`, que cerró el 2026-08-04 con los tres crates
    y está en `git log -- docs/CRATES.md`.
- `docs/BLENDER_AUTHORING_GUIDE.md` es una ayuda personal de operación, trackeada
  por conveniencia pero **no autoritativa**: el contrato sigue siendo
  `ASSET_PIPELINE.md` y el plan de animación/rig vive en sus docs de dominio.
- Ningún acuerdo de esta conversación es válido si no quedó escrito en un
  archivo del repo — la coordinación no depende de memoria de sesión.
