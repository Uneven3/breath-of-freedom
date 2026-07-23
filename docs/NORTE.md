# Norte — qué estamos construyendo

**Breath of Freedom** — acción-aventura de mundo abierto en Bevy (Rust),
GNU GPL, co-op multiplayer como objetivo base. Busca el *feeling* de
**The Legend of Zelda: Breath of the Wild** sin nada de la IP de Zelda:
mundo, historia, razas y assets propios. (≤200 líneas; este archivo es la
visión — lo táctico vive en `AHORA.md`, las reglas en `ARCHITECTURE.md`.)

## Postura legal / IP

- Cero nombres, lore, personajes, música o assets de Zelda.
- Lo "prestado" es mecánico y de sensación (escalada, stamina, glide,
  combate con peso) — no protegible por copyright.
- Referencias de tono/arte (no de assets): BotW, Genshin Impact, Monster
  Hunter Stories 3.

## Pilares

1. **Movimiento primero** — respuesta instantánea con momentum leve; la
   stamina limita el esfuerzo; escalar/nadar/planear se sienten físicos.
2. **Traversal abierto** — casi cualquier superficie es escalable, a un costo.
3. **Combate con peso** — lento y deliberado; leer al enemigo importa más
   que la velocidad de input.
4. **Exploración contemplativa** — sin urgencia narrativa impuesta.
5. **Multiplayer desde el día uno** — arquitectura multi-actor y
   host-autoritativo desde temprano; single-player es la misma simulación
   con un jugador local.
6. **GNU / comunidad** — sin monetización, todo forkeable.
7. **UI mínima** — el mundo comunica el estado.

## Mundo y narrativa

- Fantasía, sin humanos; razas inteligentes humanoides (diseño abierto).
- Sin villano ni trama central obligatoria — inspiración *Majora's Mask*:
  personajes con problemas propios que el jugador puede optar por resolver.
- Tono sereno y contemplativo.

## Dirección visual y sonora

- **Low-poly por elección, no por defecto.** Sin artista dedicado, low-poly es
  lo único cost-efficient de autorear, y a la vez es lo que corre en el piso
  objetivo: **móvil de gama media (~2021)**. La restricción de autoría y la meta
  de rendimiento coinciden. Los assets actuales (Quaternius, MegaKits, Modular
  Dungeon) son de prueba, reemplazables.
- **Dirección artística por sobre fidelidad.** La belleza es *luz + color +
  atmósfera*, no detalle geométrico ni texturas complejas; silueta legible y
  paleta coherente antes que polígonos. Referencias: Journey (norte
  aspiracional), art of rally, Gedonia, The Bloodline, BattleBit, Halo 1,
  WoW Classic, Super Mario 64.
- **PBR estilizado sobre `StandardMaterial`**, inclinado a lo plano: materiales
  mate (roughness alto, cero metal), color e iluminación mandando. Para arte
  propio: vertex-color / material plano apoyado en la luz, antes que sets PBR
  texturizados. No se usa toon shader ni outline fullscreen; shaders custom se
  reservan para diagnósticos opt-in como overdraw, nunca para el baseline.
- Assets de prototipado reemplazables mediante catálogo de presentación:
  identidad de gameplay, visual y colisión permanecen independientes.
- **Rendimiento:** 60 FPS en hardware de referencia con presupuestos medidos; en
  el piso móvil el costo real no son los polígonos sino fill-rate/overdraw,
  passes fullscreen, sombras y draw calls. Más mundo no justifica degradar la
  respuesta del movimiento.
- Música ambiental minimalista; SFX estilizados. Hasta tener audio real,
  cada punto sonoro emite un *cue* de debug (`[audio] cue: …`).

## Mecánicas (orden de prioridad)

1. **Movimiento** — traversal físico gateado por stamina. ✅ base jugable
2. **Cámara** — orbital tercera persona, modo apuntado. ✅ (lock-on pendiente)
3. **Combate** — melee con peso ✅, arco ✅, sigilo ✅ (bonus ×4), durabilidad
   de armas e inventario base ✅. Escudo/parry ⏳.
4. **Monturas** — ✅ horse base (montar, carga, inmunidad de dueño). El
   diseño final es más ambicioso: criaturas variadas, terrestres y
   voladoras, con vínculo personal jugador-criatura (línea *Avatar*:
   Ikran/Direwolf), no transporte genérico.
5. **Mundo y entorno** — ciclo día/noche ✅, mundo 320×320 + bosque ✅;
   ⏳ próximo foco: recuperar rendimiento con profiling/LOD/culling antes de
   sumar temperatura, clima, tala, animales o personajes. Después: crafteo y
   buceo.
6. **Multiplayer** — co-op host-autoritativo (contrato multi-actor ya
   implementado; red no empezada).
7. **Personajes/problemas** — quests opcionales estilo Majora's Mask.

## Detalle mecánico comprometido

- **Arco:** apuntado libre en dos fases, carga estilo Bannerlord (soltar
  rápido = flecha lenta e imprecisa), caída parabólica real.
- **Melee:** pocas armas bien diferenciadas por peso/velocidad/alcance,
  todas con durabilidad (fuerza variar el arsenal).
- **Sigilo:** multiplicador de daño en ataque sorpresa; bonus, no pilar.
- **IA enemiga:** lee al jugador — percepción gradual ✅; flanqueo,
  reacciones grupales y huida al estar heridos ⏳.
- **Traversal:** escalar ✅, planear ✅, nadar/bucear ⏳ (con oxígeno y
  corrientes, línea Fontaine de Genshin), snowboard en pendientes ⏳.
- **Clima/día-noche ⏳:** ciclo visual y noche iluminada ✅; frío/calor
  exigen preparación; lluvia moja y afecta el agarre; tormentas eléctricas
  atraen metal; la noche cambia spawns y comportamiento.
- **Crafteo ⏳:** equipo a partir de materiales del mundo (más que cocinar).

## Qué NO estamos construyendo

- Gacha, live service, battle pass.
- Assets, historia o motor de Zelda.
- Trama principal obligatoria con checklist de misiones.

## Decisiones abiertas

- Número objetivo de jugadores co-op.
- Diseño de razas (cuántas, cuáles, rasgos).
- Estructura del sistema de "problemas resolubles".
- Diseño concreto de monturas (criaturas, domado/vínculo).
- Árbol de crafteo/recetas.
- Tamaño del mundo y modelo de persistencia.
- Pipeline concreto de arte propio low-poly en **Blender** (LOD, compresión de
  texturas para móvil). *Decidido:* arte propio low-poly en Blender, no
  dependencia de CC0.
