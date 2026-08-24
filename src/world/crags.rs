//! Peñascos: rocas, paredes y acantilados **irregulares**, con collider exacto.
//!
//! Existen porque el terreno dejó de ser escalable — un heightmap es
//! `y = f(x,z)` y no se pliega, así que lo escalable tiene que ser un objeto
//! colocado encima. Y existen **generados** en vez de importados porque lo que
//! hay que probar es el sensor de escalada, no la decoración: una malla de
//! catálogo puede no tener ni una sola cara reclinada, mientras que acá el
//! desorden es un parámetro y se garantiza que esté.
//!
//! Un cubo no sirve de prueba: sus caras son planos perfectos y su normal nunca
//! cambia, así que el sensor acierta por construcción. Lo que rompe un sensor
//! son las caras que se recuestan, las que se van de la vertical y las que
//! cambian de normal entre un tick y el siguiente.
//!
//! El collider es **trimesh de la misma malla que se dibuja**, no un casco
//! convexo: un casco alisaría justamente las concavidades que hacen la prueba.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bof_domain::world::hash_unit;

use super::layout::{Anchor, settle};
use crate::asset_pipeline::materials::MaterialPalette;
use crate::scene::AppState;

const CRAG_MATERIAL: &str = "GrayboxProp";

/// Subdivisiones del icosaedro base. Cada una cuadruplica los triángulos, así
/// que 3 son 1.280 por peñasco: bastante para que la silueta tenga rasgos, poco
/// para que la dirección low-poly siga leyéndose como facetas. Es el valor de
/// las piezas de prueba; el acantilado pide una más porque es cuatro veces más
/// largo y con ésta se le verían los triángulos.
const SUBDIVISIONS: u32 = 3;

#[cfg(test)]
fn triangles_for(subdivisions: u32) -> usize {
    20 * 4_usize.pow(subdivisions)
}

/// Una pieza escalable del curso de prueba.
#[derive(Clone, Copy)]
struct CragRow {
    name: &'static str,
    /// Centro. Cómo se lee su `y` lo dice `anchor`.
    pos: Vec3,
    /// Las piezas de prueba se asientan sobre el suelo; el acantilado **no**.
    /// Mide 18 m de frente sobre relieve que sube, y `Anchor::Ground` muestrea
    /// un solo punto: entierra una punta y hace flotar la otra. Su altura se
    /// declara, y sale de la mesa que tiene que coronar.
    anchor: Anchor,
    /// Semiejes en metros: es lo que separa una roca de una pared.
    radii: Vec3,
    /// Cuánto sobresalen los bultos, **en metros y no como fracción del radio**.
    ///
    /// La versión proporcional del 2026-08-23 le daba al acantilado bultos de
    /// casi dos metros, y un bulto de dos metros sobre una pared se inclina
    /// hasta pasarse de la vertical: la pieza entera se volvía saliente. Con la
    /// medida absoluta los tres peñascos tienen la misma textura y la pendiente
    /// que agrega no depende del tamaño.
    bump_metres: f32,
    seed: u32,
    /// Cuántas veces se subdivide el icosaedro base. Por fila y no global: la
    /// densidad que hace ver a una roca de 3 m facetada deja al acantilado de
    /// 18 m con triángulos del tamaño del cuerpo del jugador.
    subdivisions: u32,
    /// Qué fracción del semieje vertical mide la cúpula de arriba.
    ///
    /// **Es lo que separa una roca de un acantilado, y sin esto no había forma
    /// de autorar el segundo.** `column` deja la mitad de abajo vertical y la de
    /// arriba como cúpula, las dos de `radii.y`; como la pieza además se
    /// entierra, la cúpula siempre le ganaba a la pared visible y toda pieza
    /// grande salía con forma de papa. Aplastando la cúpula, la pared es la que
    /// manda y arriba queda una repisa más plana, que el mantle agradece.
    cap_share: f32,
}

/// Cada cuántos metros se repite un bulto. Junto con `bump_metres` fija la
/// pendiente que el desorden puede agregar — y por eso el desorden nunca
/// convierte una pared en un saliente.
const FEATURE_METRES: f32 = 2.5;

/// El curso de escalada. Tres tamaños porque las tres preguntas son distintas:
/// si el sensor engancha una cara que se recuesta, si aguanta una pared alta
/// donde hay que subir varios cuerpos, y si encuentra la repisa de arriba.
const CRAGS: &[CragRow] = &[
    CragRow {
        name: "Roca",
        pos: Vec3::new(-14.0, 1.4, 12.0),
        radii: Vec3::new(1.8, 1.4, 1.6),
        bump_metres: 0.45,
        seed: 0x51ed_0001,
        subdivisions: SUBDIVISIONS,
        cap_share: 1.0,
        anchor: Anchor::Ground,
    },
    CragRow {
        name: "Pared",
        pos: Vec3::new(0.0, 3.6, 14.0),
        radii: Vec3::new(4.5, 3.8, 1.6),
        bump_metres: 0.55,
        seed: 0x51ed_0002,
        subdivisions: SUBDIVISIONS,
        cap_share: 1.0,
        anchor: Anchor::Ground,
    },
    CragRow {
        name: "Acantilado",
        pos: Vec3::new(20.0, 6.5, 16.0),
        radii: Vec3::new(7.0, 6.8, 3.4),
        bump_metres: 0.7,
        seed: 0x51ed_0003,
        subdivisions: SUBDIVISIONS,
        cap_share: 1.0,
        anchor: Anchor::Ground,
    },
    CRAG_CLIFF,
];

/// **El primer acantilado de verdad**, y es una sola pieza a propósito.
///
/// La versión que este archivo estuvo a punto de tener eran cuatro piezas
/// solapadas, que es como se arma un acantilado con un catálogo de rocas. Acá
/// no sirve, y la razón es del motor y no estética: cada costura entre dos
/// elipsoides es un salto de la normal de la cara, medido entre 32° y 64°, y
/// `motors::climb` filtra la normal con una constante calibrada contra una
/// perturbación de **7°**. Escalar cruzando una costura sacudiría al cuerpo
/// durante un cuarto de segundo — que es, palabra por palabra, uno de los dos
/// defectos que él reportó jugando. Una pieza sola no tiene costuras.
///
/// La variedad de silueta la dan `seed` y `bump_metres`, que existen para eso,
/// y una subdivisión más para que a 18 m de largo no se le vean las facetas.
const CRAG_CLIFF: CragRow = CragRow {
    name: "Acantilado del sur",
    // Al sur del curso: el rincón libre más grande que queda entre las cajas de
    // `layout` (z de -10 a +10) y las tres piezas de prueba (z de 12 a 16).
    // `no_two_crags_overlap` cubre lo primero desde que esta fila está en la
    // tabla; contra `layout` no hay test, y por eso el lugar se eligió mirando.
    pos: Vec3::new(0.0, CLIFF_TOP - CLIFF_RADII.y * CLIFF_CAP_SHARE, -26.0),
    radii: CLIFF_RADII,
    bump_metres: 0.7,
    seed: 0x51ed_0004,
    subdivisions: SUBDIVISIONS + 1,
    cap_share: CLIFF_CAP_SHARE,
    anchor: Anchor::World,
};

/// Semiejes del acantilado: 18 m de frente y 6 de fondo. Los 13 de alto son el
/// **largo de la columna**, no la altura de la pieza: sobre el llano deja 12 m
/// de pared y entierra 3, y sobre el relieve de Terreno —que ya estaba a 4 o 5
/// m— deja unos 8 y entierra el resto. Encima van 2,6 m de cúpula aplastada.
const CLIFF_RADII: Vec3 = Vec3::new(9.0, 13.0, 3.0);

/// Dónde corona el acantilado, en altura absoluta.
///
/// **Es la altura de la mesa que tapa** (`world::plateau`), más un margen: el
/// que mantlea tiene que quedar parado sobre terreno, no sobre el aire de al
/// lado. Enterrar el resto no es un truco de arte — evita la línea de contacto
/// perfecta que el ojo lee como "objeto apoyado", el hueco que abre el LOD del
/// terreno bajo una pieza apoyada, y tener que reasentarla al esculpir cerca.
/// Detalle y medidas en `docs/CLIFFS.md`.
const CLIFF_TOP: f32 = 12.4;
/// Cuánto de la altura de la pieza es cúpula. Ver `CragRow::cap_share`.
const CLIFF_CAP_SHARE: f32 = 0.2;

/// Lo que el curso declara al presupuesto de triángulos, que es un guardarraíl
/// de test: el contador de runtime califica lo que la cámara ve, no lo que la
/// escena contiene.
#[cfg(test)]
pub(crate) fn triangle_count() -> usize {
    CRAGS
        .iter()
        .map(|row| triangles_for(row.subdivisions))
        .sum::<usize>()
        + super::debris::triangle_count()
}

/// Dónde toca cada peñasco el suelo: centro y semiejes horizontales. Es lo que
/// el escombro necesita para caer sobre la línea de contacto y no sobre un
/// círculo inventado — la huella de una elipse cambia con el azimut, y un radio
/// fijo dejaría piedras flotando de un lado y enterradas del otro.
pub(super) fn footprints() -> impl Iterator<Item = (Vec3, Vec2, u32)> {
    CRAGS
        .iter()
        .map(|row| (row.pos, Vec2::new(row.radii.x, row.radii.z), row.seed))
}

pub(super) fn setup_crags(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    state: Res<State<AppState>>,
    ground: super::TerrainAccess,
) {
    let scene = *state.get();
    let ground = Some(&ground);
    for row in CRAGS {
        let mesh = crag_mesh(row);
        let Some(collider) = Collider::trimesh_from_mesh(&mesh) else {
            warn!("[world] {} no produjo collider; se omite", row.name);
            continue;
        };
        commands.spawn((
            DespawnOnExit(scene),
            Name::new(row.name),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(palette.handle(CRAG_MATERIAL)),
            Transform::from_translation(settle(row.pos, row.anchor, ground)),
            RigidBody::Static,
            collider,
        ));
    }
}

/// Una piedra suelta: la misma geometría, redonda y barata. Vive acá porque la
/// forma de un peñasco la decide este módulo, y `debris` sólo la reparte.
pub(super) fn small_stone_mesh(radius: f32, seed: u32) -> Mesh {
    crag_mesh(&CragRow {
        name: "Piedra",
        pos: Vec3::ZERO,
        radii: Vec3::splat(radius),
        bump_metres: radius * 0.35,
        seed,
        subdivisions: 1,
        cap_share: 1.0,
        anchor: Anchor::Ground,
    })
}

/// Una elipsoide facetada y abollada, con normales planas.
///
/// Las normales planas no son sólo dirección artística: le dan al sensor una
/// normal distinta por triángulo, que es el caso difícil de verdad — con
/// normales suavizadas la superficie miente y se comporta mejor de lo que es.
pub(super) fn crag_mesh(row: &CragRow) -> Mesh {
    let CragRow {
        radii,
        bump_metres,
        seed,
        subdivisions,
        cap_share,
        ..
    } = *row;
    let (directions, indices) = icosphere(subdivisions);
    let positions: Vec<Vec3> = directions
        .iter()
        .map(|dir| {
            // La cúpula se aplasta después de la columna y antes del bulto: es
            // altura, no forma, y el desorden tiene que seguir la superficie ya
            // proporcionada o la pared se recuesta.
            let column = column(*dir) * radii;
            let surface = if dir.y > 0.0 {
                Vec3::new(column.x, column.y * cap_share, column.z)
            } else {
                column
            };
            // El bulto empuja **hacia afuera en horizontal** mientras la
            // superficie es columna, y sólo se vuelve radial en la cúpula. Un
            // empujón radial en la parte vertical movería también la altura, y
            // ahí es donde la pared se inclinaba hasta volverse saliente.
            let outward = Vec3::new(surface.x, surface.y.max(0.0), surface.z);
            surface
                + outward.normalize_or(Vec3::Y)
                    * bump_metres
                    * lobes(surface / FEATURE_METRES, seed)
        })
        .collect();

    // Una posición por vértice de triángulo: es lo que permite que cada cara
    // tenga su propia normal sin arrastrar a sus vecinas.
    let mut faceted = Vec::with_capacity(indices.len());
    let mut normals = Vec::with_capacity(indices.len());
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [
            positions[triangle[0]],
            positions[triangle[1]],
            positions[triangle[2]],
        ];
        let normal = (b - a).cross(c - a).normalize_or(Vec3::Y);
        faceted.extend_from_slice(&[a, b, c]);
        normals.extend_from_slice(&[normal; 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    let count = faceted.len();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, faceted);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32, 0.0]; count]);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "3 × 20 × 4^SUBDIVISIONS = 3.840 vértices, fijado en tiempo de compilación"
    )]
    mesh.insert_indices(Indices::U32((0..count as u32).collect()));
    mesh
}

/// Estira la mitad de abajo de la esfera en columna, dejando la de arriba como
/// cúpula.
///
/// **Una elipsoide no sirve de pared, y el 2026-08-23 se midió por qué.** Su
/// punto más ancho está a media altura, así que todo lo que queda por debajo
/// —justo la franja donde el jugador se agarra— **se recuesta sobre él**: es
/// saliente, no pared. Y empeora con el tamaño, porque cuanto más grande el
/// peñasco, más arriba queda su cintura. Medido en la franja de agarre, de 0,5
/// a 2 m sobre la base: la roca chica daba 15% de pared y la grande **0%** —
/// exactamente el orden en que se podían escalar.
///
/// Con la columna, la franja de agarre es vertical a cualquier tamaño, y la
/// cúpula de arriba sigue dando la repisa que el mantle necesita.
fn column(dir: Vec3) -> Vec3 {
    let horizontal = Vec2::new(dir.x, dir.z);
    let on_sphere = horizontal.length();
    if on_sphere <= 1e-4 {
        return dir;
    }
    // Abajo el radio es **constante**: cualquier ensanchamiento con la altura
    // es un saliente, y una versión anterior que cerraba suavemente hacia el
    // polo dejaba al acantilado con 96% de caras salientes en la franja de
    // agarre, porque esa franja cae justo donde el estrechamiento se nota.
    // Arriba sigue siendo la esfera, que es la cúpula del mantle.
    let profile = if dir.y >= 0.0 {
        (1.0 - dir.y * dir.y).max(0.0).sqrt()
    } else {
        1.0
    };
    let widened = horizontal / on_sphere * profile;
    Vec3::new(widened.x, dir.y, widened.y)
}

/// Desorden suave y determinista sobre la esfera, en tres octavas.
///
/// Toma un punto **en unidades de rasgo** (metros divididos por
/// `FEATURE_METRES`), así que la textura es la misma en un peñasco chico que en
/// uno grande. Producto de senos y no ruido por vértice: el ruido suelto se ve
/// como sal y pimienta.
fn lobes(at: Vec3, seed: u32) -> f32 {
    const OCTAVES: u32 = 3;
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.7;
    let mut normaliser = 0.0;
    for octave in 0..OCTAVES {
        let phase = |i: u32| hash_unit(seed.wrapping_add(octave * 3 + i)) * std::f32::consts::TAU;
        total += amplitude
            * (frequency * at.x + phase(0)).sin()
            * (frequency * at.y + phase(1)).sin()
            * (frequency * at.z + phase(2)).sin();
        normaliser += amplitude;
        amplitude *= 0.5;
        frequency *= 2.3;
    }
    total / normaliser
}

/// Direcciones unitarias e índices de un icosaedro subdividido `subdivisions`
/// veces. Se genera acá y no con `Sphere::mesh().ico()` porque hace falta la
/// topología **antes** de facetar, para que el desplazamiento mueva los vértices
/// compartidos juntos y no abra grietas.
fn icosphere(subdivisions: u32) -> (Vec<Vec3>, Vec<usize>) {
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let mut vertices: Vec<Vec3> = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ]
    .into_iter()
    .map(|v| Vec3::from(v).normalize())
    .collect();

    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    for _ in 0..subdivisions {
        let mut split = Vec::with_capacity(faces.len() * 4);
        // Sin caché de aristas: los vértices duplicados en la costura se
        // desplazan igual porque el desplazamiento depende sólo de la
        // dirección, así que no hay grieta que cerrar.
        for [a, b, c] in faces {
            let mut midpoint = |i: usize, j: usize| {
                let point = ((vertices[i] + vertices[j]) / 2.0).normalize();
                vertices.push(point);
                vertices.len() - 1
            };
            let (ab, bc, ca) = (midpoint(a, b), midpoint(b, c), midpoint(c, a));
            split.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        faces = split;
    }

    let indices = faces.into_iter().flatten().collect();
    (vertices, indices)
}

#[cfg(test)]
mod tests {

    /// `cos(45°)`, o sea el `FLOOR_MIN_UP_DOT` de la simulación — el umbral que
    /// separa piso de pared. Se escribe con la constante de `std` y no con el
    /// decimal porque el decimal se despega en silencio del que manda.
    const WALKABLE_NORMAL_Y: f32 = std::f32::consts::FRAC_1_SQRT_2;

    /// **Lo que el jugador encuentra a la altura a la que se agarra.**
    ///
    /// La banda va de 0,5 a 2,0 m sobre la base del peñasco, que es donde caen
    /// los seis casts de perfil con el cuerpo parado en el suelo.
    #[test]
    fn the_grabbing_band_is_mostly_wall_with_some_overhang() {
        println!("\n[crags] caras en la banda de agarre (0,5–2,0 m sobre la base)");
        for row in CRAGS {
            let mesh = crag_mesh(row);
            // Desde la **línea del suelo**, no desde el fondo de la malla: el
            // peñasco está hundido, y medir desde su punto más bajo era medir
            // geometría enterrada — la primera versión de este reporte daba
            // 100% de saliente en las dos piezas grandes por eso.
            let base = -row.pos.y;
            let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                continue;
            };
            let positions: Vec<Vec3> = positions
                .as_float3()
                .unwrap_or(&[])
                .iter()
                .map(|p| Vec3::from(*p))
                .collect();
            let (mut overhang, mut wall, mut walkable) = (0, 0, 0);
            for face in positions.chunks_exact(3) {
                let centre = (face[0] + face[1] + face[2]) / 3.0;
                let height = centre.y - base;
                if !(0.5..=2.0).contains(&height) {
                    continue;
                }
                let normal = (face[1] - face[0])
                    .cross(face[2] - face[0])
                    .normalize_or(Vec3::Y);
                if normal.y < -0.05 {
                    overhang += 1;
                } else if normal.y < WALKABLE_NORMAL_Y {
                    wall += 1;
                } else {
                    walkable += 1;
                }
            }
            let total = overhang + wall + walkable;
            println!(
                "  {:<12} {total:>4} caras · saliente {:>3}% · pared {:>3}% · pisable {:>3}%",
                row.name,
                overhang * 100 / total.max(1),
                wall * 100 / total.max(1),
                walkable * 100 / total.max(1),
            );
            assert!(
                wall * 2 > total,
                "{}: sólo {}% de la franja de agarre es pared. Un peñasco cuya \
                 franja es saliente no se puede escalar de ninguna forma, y el \
                 2026-08-23 los tres fallaron acá antes de que nadie lo notara \
                 jugando",
                row.name,
                wall * 100 / total.max(1)
            );
            assert!(
                overhang > 0,
                "{}: sin una sola cara saliente no hay nada que estresar; eso es \
                 un cilindro, no un peñasco",
                row.name
            );
        }
    }
    use super::*;

    #[test]
    fn every_crag_has_a_unique_name_and_real_size() {
        let mut names: Vec<&str> = CRAGS.iter().map(|row| row.name).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "dos peñascos comparten nombre");
        for row in CRAGS {
            assert!(
                row.radii.cmpgt(Vec3::ZERO).all(),
                "{} es degenerado",
                row.name
            );
            assert!(
                (0.0..1.0).contains(&row.bump_metres),
                "{}: un desorden de 1 o más invierte la superficie",
                row.name
            );
        }
    }

    #[test]
    fn the_declared_triangle_count_matches_the_mesh() {
        for row in CRAGS {
            let mesh = crag_mesh(row);
            assert_eq!(
                mesh.count_vertices(),
                triangles_for(row.subdivisions) * 3,
                "{}: el presupuesto mentiría",
                row.name
            );
        }
    }

    fn vertices_of(mesh: &Mesh) -> Vec<Vec3> {
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attribute| attribute.as_float3().map(<[[f32; 3]]>::to_vec))
            .unwrap_or_default()
            .into_iter()
            .map(Vec3::from)
            .collect()
    }

    /// **La prueba de que no es un cuadrado**, y de que el desorden mide lo que
    /// dice medir: cada vértice se aparta de la columna lisa como mucho
    /// `bump_metres`, y alguno se aparta de verdad.
    #[test]
    fn the_bump_is_measured_in_metres_and_actually_lands() {
        let row = CRAGS[1];
        let bump = row.bump_metres;
        let bumpy = vertices_of(&crag_mesh(&row));
        let smooth = vertices_of(&crag_mesh(&CragRow {
            bump_metres: 0.0,
            ..row
        }));
        let offsets: Vec<f32> = bumpy
            .iter()
            .zip(&smooth)
            .map(|(rough, plain)| (*rough - *plain).length())
            .collect();
        let worst = offsets.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            worst <= bump + 1e-3,
            "un bulto de {worst} m pasa el máximo declarado de {bump} m"
        );
        assert!(
            worst > bump * 0.5,
            "el bulto más grande fue {worst} m: el desorden no está llegando"
        );
    }

    /// El collider es la malla que se ve. Si esto falla, el jugador escala algo
    /// distinto de lo que mira.
    #[test]
    fn every_crag_produces_a_collider() {
        for row in CRAGS {
            let mesh = crag_mesh(row);
            assert!(
                Collider::trimesh_from_mesh(&mesh).is_some(),
                "{} no produjo collider",
                row.name
            );
        }
    }

    /// Los tres tienen que estar separados: dos peñascos que se tocan forman
    /// una concavidad que ninguno de los dos autoró, y el sensor de escalada
    /// deja de estar probando lo que dice el nombre de la fila.
    #[test]
    fn no_two_crags_overlap() {
        for (index, row) in CRAGS.iter().enumerate() {
            for other in &CRAGS[index + 1..] {
                let gap = (row.pos - other.pos).abs();
                let touching = row.radii
                    + Vec3::splat(row.bump_metres)
                    + other.radii
                    + Vec3::splat(other.bump_metres);
                assert!(
                    gap.cmpgt(touching).any(),
                    "{} y {} se solapan",
                    row.name,
                    other.name
                );
            }
        }
    }

    /// La cara vertical tiene que ser **más alta que el alcance del mantle**, o
    /// el jugador la sube sin escalar y la prueba no prueba nada.
    #[test]
    fn the_wall_and_the_cliff_are_taller_than_a_mantle() {
        let reach = bof_domain::world::MAX_UNWALKABLE_RISE_METRES;
        for row in CRAGS.iter().filter(|row| row.name != "Roca") {
            assert!(
                row.radii.y * 2.0 > reach,
                "{} mide {} m de alto y el mantle llega a {reach} m",
                row.name,
                row.radii.y * 2.0
            );
        }
    }
}
