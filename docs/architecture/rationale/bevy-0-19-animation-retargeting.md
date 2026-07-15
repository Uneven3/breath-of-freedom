# Rationale: Bevy 0.19 Animation Retargeting & Manual Bone Linking

Este documento detalla la investigación y las decisiones tomadas para lograr que un modelo GLTF sin animaciones internas (`Knight.glb`) reproduzca clips de animación de un archivo separado (`Rig_Medium_MovementBasic.glb`) de forma fluida en Bevy 0.19.0.

## 1. El Problema: T-Pose y Silencio en el Pipeline

Al cargar un modelo GLTF estático (sin animaciones internas), Bevy no instancia el componente `AnimationPlayer` automáticamente en ninguna parte de la jerarquía.
Al intentar instanciarlo de forma manual en la raíz (`WorldAssetRoot`), el modelo permanecía en **T-pose** perpetuo sin arrojar ningún warning ni error.

Esto se debió a dos fallos fundamentales en el pipeline de animación de Bevy 0.15+:
1. **Desalineación de Rutas (Hierarchy Mismatch)**: Las curvas de la animación de KayKit están grabadas relativas al nodo de la escena (`Rig_Medium/root/hips/...`). Colocar el `AnimationPlayer` en el nodo raíz de Bevy (`WorldAssetRoot`) hacía que las rutas relativas no coincidieran (ej. buscando `Rig_Medium` directamente bajo el padre, pero habiendo un nodo intermedio `Scene`).
2. **Falta de Componentes de Enlace (Linking Components)**: Bevy 0.15+ requiere que cada entidad que vaya a ser animada (huesos del esqueleto y mallas) posea un componente `AnimationTargetId` (el hash UUID de su ruta de nombres) y un componente `AnimatedBy(Entity)` que apunte directamente a la entidad que posee el `AnimationPlayer`. Si el archivo GLTF original no tiene animaciones, el cargador de Bevy no inserta estos componentes, por lo que las articulaciones quedan "invisibles" para el motor de animación.

## 2. Solución Arquitectónica

### A. Programación del Grafo de Animación (`AnimationGraph`)
En Bevy 0.19, no se pueden reproducir clips directamente desde un `AnimationPlayer` sin un grafo activo. Construimos el grafo programáticamente en un sistema de inicialización una vez que los GLTF de animación están listos:

```rust
let mut graph = AnimationGraph::new();
let idle = graph.add_clip(idle_clip, 1.0, graph.root);
let walk = graph.add_clip(walk_clip, 1.0, graph.root);
let run = graph.add_clip(run_clip, 1.0, graph.root);
```

Registramos los `NodeIndex` devueltos en un recurso global `PlayerAnimations` para poder invocarlos por su ID de nodo en lugar de por handle del clip.

### B. Posicionamiento en el Nodo `"Scene"`
Identificamos mediante introspección que Bevy instancia el nodo `"Scene"` como un hijo directo del `WorldAssetRoot`. Colocar el `AnimationPlayer` en este nodo `"Scene"` hace que las rutas de los huesos en la animación (`Rig_Medium/root/hips/...`) se alineen de forma idéntica con la jerarquía del Caballero.

### C. Enlace Manual de Huesos (Bone Linking)
Escribimos una función recursiva que recorre el subárbol de `"Scene"` y añade los componentes necesarios en base a los nombres acumulados en la ruta de descendencia:

```rust
fn link_descendants(
    commands: &mut Commands,
    entity: Entity,
    path: Vec<Name>,
    children_query: &Query<&Children>,
    names_query: &Query<&Name>,
    player_entity: Entity,
) {
    let mut new_path = path;
    if let Ok(name) = names_query.get(entity) {
        new_path.push(name.clone());
        let target_id = bevy::animation::AnimationTargetId::from_names(new_path.iter());
        commands.entity(entity).insert((
            bevy::animation::AnimatedBy(player_entity),
            target_id,
        ));
    }
    // ... recursión en children ...
}
```

Esto inyecta `AnimatedBy(scene_entity)` y `AnimationTargetId` con los hashes UUID correspondientes a la ruta real de cada hueso.

### D. Prevención del Bucle de Transición (Animation Freeze)
Gatillar `transitions.play(...)` incondicionalmente cada frame del bucle `Update` provoca que la animación se reinicie continuamente a su frame inicial de cross-fade, congelándola visualmente.

Para evitar esto, se implementó una condición de guarda:
- Solo se llama a `transitions.play(...)` si la animación objetivo **no** está activa: `!player.is_playing_animation(target_node)`.
- Si ya está activa, simplemente modulamos dinámicamente la velocidad de la caminata con `player.animation_mut(target_node).unwrap().set_speed(multiplier)`.

## 3. Lecciones Aprendidas y Estándares de Assets (Best Practices)

Hacer esta prueba con assets de diferentes fuentes nos enseñó principios importantes sobre el flujo de assets en Bevy 0.19:

### A. Empaquetado Óptimo de Assets
*   **Problema de archivos separados:** Cargar el modelo 3D por un lado (`Knight.glb`) y las animaciones por otro (`Rig_Medium_MovementBasic.glb`) nos obligó a realizar un enlace manual a bajo nivel de los huesos en código de Rust.
*   **Mejor Práctica (Recomendado):** Para evitar la sobrecarga de programar enlazadores manuales de huesos, los personajes deben exportarse desde Blender (o software DCC) con **todas sus animaciones embebidas directamente en el mismo archivo `.glb`** (ej. un solo `Knight.glb` con múltiples tracks). De esta forma, el cargador de GLTF de Bevy autogenera todos los componentes `AnimatedBy` y `AnimationTargetId` sin intervención de código.

### B. Serialización del Grafo de Animación (`.animgraph.ron`)
*   Aunque construir el grafo programáticamente (`AnimationGraph::new()`) funciona bien para inicializaciones dinámicas o prototipado, para proyectos en producción se recomienda **serializar el grafo en archivos `.animgraph.ron`**. Esto desacopla las configuraciones de mezcla, máscaras de huesos y transiciones de la lógica del código compilado en Rust.

### C. La Jerarquía de Bevy 0.19 (`ChildOf`)
*   Se aprendió que en Bevy 0.19 la jerarquía de entidades ya no utiliza el componente `Parent`. Ha sido reemplazado por la relación/componente **`ChildOf`**, y la entidad padre debe obtenerse mediante el método `.parent()` sobre el componente `ChildOf` en lugar de `.get()`.

