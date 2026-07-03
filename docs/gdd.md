# Breath of Freedom — Game Design Document

**Motor:** Bevy (Rust)
**Licencia:** GNU GPL
**Género:** Acción-aventura de mundo abierto, co-op multiplayer

---

## 1. Visión

Un juego de acción-aventura en mundo abierto que busca el *feeling* de
**The Legend of Zelda: Breath of the Wild** — exploración serena y
contemplativa, a tu ritmo — reconstruido como proyecto open-source con
multiplayer co-op como objetivo base y arquitectura preparada desde temprano.
Mundo, historia, personajes, razas y assets son **propios**. No hay nada de
Zelda salvo la inspiración mecánica y de sensación de juego.

## 2. Postura legal / IP

- Cero nombres, lore, personajes, música o assets de Zelda. Todo se diseña de cero.
- Lo que se toma "prestado" es puramente mecánico y de sensación de juego
  (física de escalada, stamina, glide, combate con peso), que no es
  protegible por copyright.
- Referencias de tono/arte/sensación (no de assets ni de historia): **BotW**,
  **Genshin Impact**, **Monster Hunter Stories 3**.
- El nombre del proyecto, el mundo, las razas y la narrativa son originales.

## 3. Pilares

| Pilar | Descripción |
|---|---|
| **Movimiento primero** | Respuesta instantánea al input, con momentum leve. La stamina limita el esfuerzo. Escalar, nadar y planear se sienten físicos. |
| **Traversal abierto** | Casi cualquier superficie es escalable, a un costo. |
| **Combate con peso** | Lento y deliberado, no frenético. Leer al enemigo importa más que la velocidad de input. |
| **Exploración contemplativa** | Sin urgencia narrativa impuesta. El mundo se recorre a gusto propio. |
| **Multiplayer desde el día uno** | La arquitectura se diseña desde temprano para múltiples actores y host-autoritativo; single-player es la misma simulación con un jugador local. |
| **GNU / comunidad** | Sin monetización, sin assets propietarios. Todo forkeable. |
| **UI mínima** | El mundo comunica el estado; la pantalla se mantiene limpia. |

## 4. Mundo y narrativa

- **Fantasía, sin humanos.** Todas las razas inteligentes son humanoides;
  las razas concretas son una decisión abierta.
- **Sin villano ni gancho central obligatorio.** Inspiración *Majora's Mask*:
  el mundo está poblado de personajes con problemas propios que el jugador
  puede optar por resolver, en vez de una trama principal que empuja al
  jugador de misión en misión.
- Tono sereno y contemplativo, no dramático.

## 5. Dirección visual

- **Estilo:** cel-shaded, con iluminación realista sobre el shading estilizado
  (mismo approach que BotW/Genshin: modelos toon + luz dinámica creíble).
- **Paleta:** vibrante y saturada, tipo BotW.
- **Referencias:** BotW, Genshin Impact, Monster Hunter Stories 3.

## 6. Dirección sonora

- **Música:** ambiental y minimalista, tipo BotW (silencio + motivos sueltos,
  no cama orquestal constante).
- **SFX:** estilizados, no realistas.
- **Composición/mezcla final: diferida.** Hasta que se produzca audio real,
  todo punto del juego donde debería sonar música o un SFX específico debe
  emitir un *cue* identificable por un placeholder de debug (ej. log de
  consola `[audio] cue: snow_ambient`), desacoplando la lógica del asset real.

## 7. Combate

- **Sensación:** lento y con peso, timing deliberado — el feeling de BotW,
  no un character-action rápido.
- **IA enemiga:** lee al jugador (flanqueo, reacciones grupales, huida cuando
  están heridos), como en BotW.
- **Armas con durabilidad:** se rompen con el uso, fuerza variar el arsenal.
- **Sigilo:** multiplicador de daño en ataque sorpresa; es un bonus, no un
  pilar aparte.

## 8. Armas y equipo

- **Arco:** apuntado libre, cámara en primera persona / tiempo ralentizado al
  apuntar — como BotW.
- **Cuerpo a cuerpo:** pocos tipos de espada/arma bien diferenciados por
  peso/velocidad/alcance (no un catálogo extenso tipo BotW), todas con
  durabilidad.
- **Monturas:** variadas, terrestres y voladoras, con vínculo personal
  jugador-criatura — en la línea de *Avatar* (Na'vi + Ikran/Banshee,
  Direwolf/Palulukan). No es solo transporte genérico tipo "caballo".
- **Crafteo:** sistema con más profundidad que solo cocinar — crafteo de
  equipo a partir de materiales recolectados en el mundo.

## 9. Traversal y actividades

- **Escalar:** traversal físico gateado por stamina.
- **Planear:** glide con control aéreo y consumo de stamina.
- **Nadar:** exploración acuática con profundidad, en la línea de la
  expansión de Fontaine en Genshin Impact — buceo, aliento/oxígeno,
  corrientes y visibilidad bajo el agua, no solo cruzar de un punto a otro.
- **Deslizar:** snowboard en nieve/pendientes.

## 10. Clima y ciclo día/noche

Afectan mecánicas desde el diseño base, como en BotW:
- Frío/calor requieren preparación (equipo/elixires) o dañan al jugador.
- Lluvia moja superficies y afecta el agarre al escalar.
- Tormentas eléctricas atraen metal (armas, armaduras).
- El ciclo día/noche cambia el comportamiento y spawn de enemigos.

## 11. Mecánicas (orden de prioridad)

1. **Movimiento** — traversal físico gateado por stamina.
2. **Cámara** — orbital tercera persona, modo apuntado, lock-on.
3. **Combate** — melee con peso, arco, sigilo, durabilidad de armas.
4. **Monturas** — criaturas variadas, terrestres y voladoras.
5. **Mundo y entorno** — clima, ciclo día/noche, crafteo, buceo/nado profundo.
6. **Multiplayer** — co-op host-autoritativo, sin servidor dedicado y sin límite fijo de jugadores decidido.
7. **Sistema de personajes/problemas** — estructura tipo *Majora's Mask* para quests opcionales.

## 12. Qué NO estamos construyendo

- Gacha, live service, battle pass.
- Assets, historia o motor propietario/de Zelda.
- Trama principal obligatoria con marcadores de misión estilo checklist.

## 13. Decisiones abiertas

- Número objetivo de jugadores simultáneos para multiplayer co-op.
- Diseño concreto de razas (cuántas, cuáles, rasgos).
- Estructura del sistema de "problemas resolubles" tipo Majora's Mask.
- Diseño concreto de monturas (qué criaturas, terrestres vs. voladoras, cómo se vinculan/doman).
- Diseño concreto del árbol de crafteo/recetas de equipo.
- Tamaño del mundo y modelo de persistencia.
- Pipeline de assets (¿solo CC0? ¿arte propio?).
