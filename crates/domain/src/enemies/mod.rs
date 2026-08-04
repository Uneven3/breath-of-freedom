pub mod perception;
pub mod state;

use bevy_ecs::prelude::*;

/// Marker for an AI-controlled actor, analogous to `Player`.
///
/// Dato, no sistema: presentación lo consulta para elegir el visual de un
/// bokobo y para pintar su barra de vida, y con las capas hermanas eso sólo
/// puede leerse desde acá.
#[derive(Component)]
pub struct Enemy;

/// Pedido de presencia del par graybox. Lo escribe quien compone la escena y el
/// hub de debug; simulación decide cómo se construye un bokobo.
#[derive(Message, Debug, Clone, Copy)]
pub enum BokoboSpawnRequest {
    /// A scene requires the pair; leave an existing pair alone.
    Ensure,
    /// El hub de debug invierte explícitamente la presencia actual.
    Toggle,
}
