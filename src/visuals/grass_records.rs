//! La brizna como **registro**, no como geometría.
//!
//! Cada nivel de la pradera tiene acá tres cosas: una **malla índice** que todos
//! sus chunks comparten, un **buffer** con un registro de 16 bytes por brizna, y
//! un libro de **casilleros** que dice qué chunk ocupa cuál. El vertex shader
//! junta las tres: `MeshTag` le da el casillero, `vertex_index` le da la brizna y
//! la esquina, y de ahí construye los vértices que antes venían horneados.
//!
//! Existe aparte de `grass.rs` porque son dos preguntas distintas: aquél decide
//! **qué se planta**, esto **cómo llega a la GPU**.
//!
//! # Lo que hace que el batching funcione
//!
//! El batching automático de Bevy exige el **mismo `Handle<Mesh>`**, y una malla
//! por chunk lo impedía. Acá todos los chunks de un nivel comparten una, y eso se
//! puede porque **todos tienen exactamente la misma cantidad de briznas**:
//! `blades_per_chunk` depende del nivel y de las perillas, no de la celda. De la
//! misma propiedad sale que el casillero sea un índice con stride fijo y no un
//! rango — sin ella haría falta un allocator con fragmentación.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;

/// Cuánto pesa un registro: cuatro `f32`.
///
/// Contra los **136 bytes** que pesaba una hoja horneada (cuatro vértices de 28 B
/// más seis índices de 4). Ése es el canje del Paso 2.
pub(super) const RECORD_BYTES: usize = 16;

/// Un registro, en el orden que `BladeRecord` declara en `grass.wgsl`.
///
/// **Las dos definiciones son la misma cosa y no hay nada que las ate**, así que
/// se tocan juntas. `xy` es la base en XZ de mundo, `z` la altura del terreno
/// debajo, y `w` el alcance del anillo con la altura de la brizna en la fracción.
pub(super) fn blade_record(xz: Vec2, ground_y: f32, packed: f32) -> [f32; 4] {
    [xz.x, xz.y, ground_y, packed]
}

/// Los casilleros de un nivel: qué chunk vive en cuál.
#[derive(Default)]
struct SlotBook {
    free: Vec<u32>,
    next: u32,
}

impl SlotBook {
    fn take(&mut self) -> u32 {
        self.free.pop().unwrap_or_else(|| {
            let slot = self.next;
            self.next += 1;
            slot
        })
    }

    fn clear(&mut self) {
        self.free.clear();
        self.next = 0;
    }
}

/// Lo que un nivel tiene en la GPU.
#[derive(Default)]
pub(super) struct RingRecords {
    /// La malla que comparten todos sus chunks. `None` hasta que se sabe cuántas
    /// briznas lleva uno, que depende de las perillas.
    pub mesh: Option<Handle<Mesh>>,
    pub buffer: Handle<ShaderBuffer>,
    /// El buffer en CPU. Crece hasta la marca de agua de casilleros y no se
    /// encoge: rodar la grilla los recicla, así que se estabiliza en cuanto el
    /// jugador dio una vuelta.
    data: Vec<u8>,
    slots: SlotBook,
    by_chunk: HashMap<IVec2, u32>,
    /// Cuántas briznas lleva un chunk de este nivel. Es el stride del buffer y
    /// también lo que el shader necesita para ubicar un registro.
    pub stride: u32,
    dirty: bool,
}

impl RingRecords {
    /// Un nivel recién nacido, con su buffer y sin briznas.
    pub fn new(buffer: Handle<ShaderBuffer>) -> Self {
        Self {
            buffer,
            ..Self::default()
        }
    }

    /// Tira todo lo que dependía de la configuración anterior.
    ///
    /// Lo llama quien mueve una perilla: la malla índice tiene tantos vértices
    /// como briznas lleva un chunk, así que un cambio de densidad la invalida
    /// entera — igual que invalidaba los chunks horneados.
    pub fn reset(&mut self) {
        self.mesh = None;
        self.data.clear();
        self.slots.clear();
        self.by_chunk.clear();
        self.stride = 0;
        self.dirty = true;
    }

    /// El casillero de un chunk, tomando uno libre si es nuevo.
    pub fn slot_for(&mut self, cell: IVec2) -> u32 {
        match self.by_chunk.get(&cell) {
            Some(slot) => *slot,
            None => {
                let slot = self.slots.take();
                self.by_chunk.insert(cell, slot);
                slot
            }
        }
    }

    /// Suelta el casillero de un chunk que se va. Sin esto el buffer sólo
    /// crecería: la grilla rueda todo el tiempo.
    pub fn release(&mut self, cell: IVec2) {
        if let Some(slot) = self.by_chunk.remove(&cell) {
            self.slots.free.push(slot);
        }
    }

    /// Copia los registros de un chunk a su casillero.
    pub fn write(&mut self, slot: u32, records: &[[f32; 4]]) {
        let stride = self.stride as usize;
        let needed = (slot as usize + 1) * stride * RECORD_BYTES;
        if self.data.len() < needed {
            self.data.resize(needed, 0);
        }
        let start = slot as usize * stride * RECORD_BYTES;
        for (index, record) in records.iter().take(stride).enumerate() {
            let at = start + index * RECORD_BYTES;
            for (lane, value) in record.iter().enumerate() {
                self.data[at + lane * 4..at + lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        self.dirty = true;
    }

    /// Sube al GPU lo que se escribió, y sólo si cambió.
    pub fn upload(&mut self, buffers: &mut Assets<ShaderBuffer>) {
        if !self.dirty || self.data.is_empty() {
            return;
        }
        let Some(mut buffer) = buffers.get_mut(&self.buffer) else {
            return;
        };
        buffer.buffer_description.size = self.data.len() as u64;
        buffer.data = Some(self.data.clone());
        self.dirty = false;
    }

    /// Cuántos bytes ocupa este nivel en el buffer. **El inventario de la escena
    /// cuenta mallas y no `ShaderBuffer`s**, así que sin este número la memoria
    /// que el Paso 2 mudó de una cosa a la otra no la ve nadie.
    pub fn buffer_bytes(&self) -> usize {
        self.data.len()
    }

    pub fn chunks(&self) -> usize {
        self.by_chunk.len()
    }
}

/// Cuántos vértices reserva la malla índice por brizna: **cuatro para todas las
/// formas, incluida la púa que sólo usa tres.**
///
/// Uniforme a propósito. Con dos zancadas conviviendo, el shader deduce la suya
/// de un uniforme, y en cuanto ese número no coincide con la malla del draw los
/// índices dejan de agrupar los vértices de una misma brizna: salen triángulos
/// que cruzan el campo. Que no pueda pasar cuesta un vértice sin usar por púa en
/// una malla que existe una vez por nivel, y ni un triángulo.
pub(super) const VERTICES_PER_BLADE: u32 = 4;

/// La malla **índice** de un nivel: la comparten todos sus chunks.
///
/// No lleva posiciones útiles —salen del registro— pero sí los tres atributos:
/// los shader defs salen de qué atributos tiene la malla, y sin `UV_0`/`UV_1` el
/// `VertexOutput` de Bevy pierde campos que el fragment usa.
pub(super) fn ring_index_mesh(blades: u32, triangles_per_blade: u32) -> Mesh {
    let vertices = blades as usize * VERTICES_PER_BLADE as usize;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; vertices]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; vertices]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0_f32; 2]; vertices]);
    let mut indices: Vec<u32> = Vec::new();
    for blade in 0..blades {
        let first = blade * VERTICES_PER_BLADE;
        // El orden de esquinas que el vertex shader deduce de `vertex_index % 4`;
        // ver `blade_vertex` en `grass.wgsl`. La púa gasta un triángulo y deja su
        // cuarto vértice sin referenciar.
        indices.extend_from_slice(&[first, first + 1, first + 2]);
        if triangles_per_blade == 2 {
            indices.extend_from_slice(&[first + 1, first + 3, first + 2]);
        }
    }
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un casillero liberado se reusa, o el buffer crece sin techo mientras el
    /// jugador camina — que es el modo de falla que este libro existe para
    /// evitar.
    #[test]
    fn a_released_slot_is_handed_out_again() {
        let mut ring = RingRecords::default();
        let first = ring.slot_for(IVec2::new(0, 0));
        let second = ring.slot_for(IVec2::new(1, 0));
        assert_ne!(first, second);
        ring.release(IVec2::new(0, 0));
        assert_eq!(ring.slot_for(IVec2::new(9, 9)), first);
    }

    /// El mismo chunk pide el mismo casillero: si cambiara, sus briznas saltarían
    /// a las de otro chunk de un frame al otro.
    #[test]
    fn a_chunk_keeps_its_slot() {
        let mut ring = RingRecords::default();
        let slot = ring.slot_for(IVec2::new(3, -2));
        assert_eq!(ring.slot_for(IVec2::new(3, -2)), slot);
    }

    /// Los registros de un chunk caen en su casillero y en ningún otro.
    #[test]
    fn records_land_in_their_own_slot() {
        let mut ring = RingRecords {
            stride: 2,
            ..RingRecords::default()
        };
        ring.write(1, &[[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]]);
        assert_eq!(ring.data.len(), 2 * 2 * RECORD_BYTES);
        // El casillero 0 quedó en cero y el 1 tiene lo escrito.
        assert!(ring.data[..2 * RECORD_BYTES].iter().all(|byte| *byte == 0));
        assert_eq!(
            f32::from_le_bytes(
                ring.data[2 * RECORD_BYTES..2 * RECORD_BYTES + 4]
                    .try_into()
                    .unwrap()
            ),
            1.0,
        );
    }

    /// La malla índice tiene un triángulo por púa y dos por hoja, y sus índices
    /// nunca salen del rango de vértices que declara.
    #[test]
    fn the_index_mesh_indexes_only_vertices_it_has() {
        for (triangles_per_blade, triangles) in [(1_u32, 1_usize), (2, 2)] {
            let mesh = ring_index_mesh(5, triangles_per_blade);
            let vertices = u32::try_from(mesh.count_vertices()).expect("malla chica de test");
            assert_eq!(vertices, 5 * VERTICES_PER_BLADE);
            let Some(Indices::U32(indices)) = mesh.indices() else {
                panic!("la malla índice tiene que traer índices u32");
            };
            assert_eq!(indices.len(), 5 * triangles * 3);
            assert!(
                indices.iter().all(|index| *index < vertices),
                "un índice apunta fuera de la malla",
            );
        }
    }
}
