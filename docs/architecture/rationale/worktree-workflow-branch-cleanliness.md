# Rationale: Limpieza de ramas y autoconsistencia del ticket en el workflow de Worktrees (antigravity)

## El problema

En la propuesta inicial del sistema de tickets y worktrees (`WORKTREE-WORKFLOW.md` y `TICKET-TEMPLATE.md`), se identificaron dos omisiones lógicas en el flujo de trabajo con Git:

1. **Ramas locales huérfanas:** El comando `git worktree remove` elimina únicamente la carpeta física del worktree de la máquina del programador, pero **no elimina** la rama local de Git asociada (ej. `feature/<slug>`). Si el programador o el agente asume que remover el worktree limpia todo, el repositorio se llenará progresivamente de decenas de ramas fusionadas inactivas, dificultando la navegación del historial.
2. **Exclusión de modificación en el alcance:** El checklist del template de tickets es estricto e indica que el agente no debe tocar ningún archivo fuera de la sección "Alcance (File Touches)". Sin embargo, el template no listaba el propio archivo de ticket (`docs/tickets/<slug>.md`). Esto creaba una contradicción de auto-cumplimiento: para marcar las tareas del ticket como completadas o documentar el progreso, el agente tenía que modificar el archivo del ticket, lo cual violaba la regla estricta de "File Touches".

---

## La decisión

Se aplican dos ajustes específicos para garantizar la limpieza del repositorio y la consistencia del flujo de agentes autónomos:

1. **Paso de eliminación de ramas en `WORKTREE-WORKFLOW.md`:**
   Se añade explícitamente el paso para eliminar la rama local fusionada tras remover el worktree:
   ```bash
   git branch -d feature/<slug>
   ```
   Esto asegura que tanto la carpeta física como el puntero de Git se limpien completamente una vez que el usuario consolida los cambios en `main`.

2. **Inclusión del propio ticket en `TICKET-TEMPLATE.md`:**
   Se añade explícitamente `- docs/tickets/<slug>.md (este archivo)` en la lista de archivos que el ticket permite modificar. De esta manera, el agente puede marcar los checklists de "Definición de terminado" y actualizar su propio progreso sin incurrir en una violación de alcance de la Constitución.

---

## Consecuencia

El flujo de trabajo es ahora auto-consistente y escalable para múltiples agentes paralelos. La base de datos de Git y el árbol de directorios se mantienen limpios a lo largo de los sprints de desarrollo.
