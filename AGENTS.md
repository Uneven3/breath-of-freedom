# AGENTS.md

**Breath of Freedom** — juego de acción-aventura de mundo abierto en Bevy
(Rust), open-source (GNU GPL), que busca el *feeling* de Breath of the Wild
sin nada de la IP de Zelda (mundo, historia, razas y assets propios).

Ver `docs/gdd.md` para la visión completa del juego.

## Stack

- Rust + Bevy 0.19 + Avian3D 0.7.0 (física).
- `cargo check` / `cargo build` / `cargo test` desde la raíz del repo.

## Cómo se construye este proyecto

Este proyecto se desarrolla con **múltiples agentes/herramientas de IA en
paralelo** (Claude, Codex, Antigravity, DeepSeek, …), muchas veces en
worktrees separados y sesiones sin memoria compartida entre sí. Por eso:

- Las reglas de código (arquitectura, ECS idiomático, qué está permitido)
  viven en `docs/CONSTITUTION.md`.
- El protocolo de coordinación entre agentes (tickets, quién está trabajando
  en qué) vive en `docs/tickets/`.
- Ningún acuerdo de esta conversación es válido si no quedó escrito en un
  archivo del repo — la coordinación no depende de memoria de sesión.

## Flujo de diseño/implementación

`.agent/skills/` contiene el set de skills para diseñar e implementar
features (`design-brief`, `implement-feature`, `auditor`, etc.). Leer ahí
antes de arrancar una feature nueva.
