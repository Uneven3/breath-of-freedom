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

- La documentación es exactamente **cuatro archivos** con presupuesto duro
  (≤1000 líneas totales — el código documenta lo que se hizo):
  `docs/ARCHITECTURE.md` (leyes §1-§20 + arquitectura y rationale, ≤200),
  `docs/NORTE.md` (visión y roadmap, ≤200) y `docs/AHORA.md` (el trabajo
  presente, ≤500), más `docs/ASSET_PIPELINE.md` (contrato Blender→Bevy,
  ≤250). No se crean otros archivos de documentación; lo cerrado se borra de
  AHORA.md (queda en git).
- Ningún acuerdo de esta conversación es válido si no quedó escrito en un
  archivo del repo — la coordinación no depende de memoria de sesión.

## Flujo de diseño/implementación

`.agent/skills/` contiene el set de skills para diseñar e implementar
features (`design-brief`, `implement-feature`, `auditor`, etc.). Leer ahí
antes de arrancar una feature nueva.
