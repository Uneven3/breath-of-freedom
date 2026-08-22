//! Leyes de `ARCHITECTURE.md` que el compilador todavía no puede cobrar.
//!
//! C2 (solo `input` lee hardware) y §12 (sin `unsafe`) son verificables por
//! máquina, y hasta hoy no las verificaba nadie: C2 se documentó el 2026-07-25
//! con 14 archivos, llegó a 15 el 2026-08-01 y desde entonces empezó a bajar.
//! Una ley que solo vive en prosa no es una ley.
//!
//! Se pensó como andamiaje hasta que Cargo cobrara la ley, y **no se borró**:
//! eso valía si `input` cruzaba a `bof_simulation`, y se quedó en la app porque
//! también maneja el cursor de la ventana. Cargo cobra que la simulación no lea
//! hardware; dentro de la app no llega, y acá es donde `HARDWARE_DEBT` se
//! congela — sólo puede encoger, y el test falla tanto si aparece un infractor
//! nuevo como si un archivo de la lista deja de serlo sin sacarlo de ella.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

/// Lo que significa "leer hardware". Son los tipos por los que el input crudo
/// entra a un sistema; `KeyCode` a secas no está porque es un valor, no una
/// lectura, y aparece legítimamente en tablas de bindings.
const HARDWARE: &[&str] = &[
    "ButtonInput<",
    "AccumulatedMouseMotion",
    "AccumulatedMouseScroll",
    "MouseMotion",
    "MouseWheel",
    "MouseButtonInput",
    "KeyboardInput",
    "GamepadButton",
    "Touches",
];

/// La deuda C2 al 2026-08-01, congelada archivo por archivo.
///
/// **Esta lista solo encoge.** Cada entrada es un sitio que lee el hardware por
/// su cuenta en vez de consumir `ActiveActions`, y por eso una tecla ya usada
/// no da error: da una función que no hace nada. Al pagar una, se borra su
/// línea; agregar una línea nueva es agregar deuda a mano, y eso se discute en
/// la revisión, no se cuela.
const HARDWARE_DEBT: &[&str] = &[
    "src/camera/freecam.rs",
    "src/debug/console.rs",
    "src/editor/brush.rs",
    "src/editor/history.rs",
    "src/editor/instances.rs",
    "src/editor/mod.rs",
    "src/editor/paint.rs",
    "src/editor/persist.rs",
    "src/presentation/debug_ui/hud_menu.rs",
    "src/presentation/debug_ui/mod.rs",
    "src/presentation/inventory_ui/mod.rs",
    "src/scene/menu.rs",
    "src/scene/mod.rs",
];

/// Los tests sí pueden fabricar input: simular una tecla es la única forma de
/// probar un sistema que la consume. El corte es **el módulo de tests**, no el
/// primer `#[cfg(test)]` que aparezca.
///
/// La diferencia importa y costó un falso negativo: `world/forest.rs` tiene un
/// `#[cfg(test)]` sobre una función a media altura, y con el corte anterior
/// todo lo que venía después quedaba invisible para este test y para el de
/// `unsafe`. Un atributo suelto arriba del archivo alcanzaba para cegar la ley
/// entera — justo lo que una ley automatizada no puede permitirse.
fn production_source(contents: &str) -> &str {
    let module = [
        "#[cfg(test)]\nmod ",
        "#[cfg(test)]\npub mod ",
        "#[cfg(test)]\npub(crate) mod ",
    ]
    .iter()
    .filter_map(|marker| contents.find(marker))
    .min();
    match module {
        Some(index) => &contents[..index],
        None => contents,
    }
}

fn source_files() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Every production crate. Most app-only laws intentionally use `source_files`;
/// repository-wide laws such as §15 must not stop at the presentation crate.
fn all_source_files() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for source in [
        root.join("src"),
        root.join("crates/domain/src"),
        root.join("crates/simulation/src"),
    ] {
        collect(&source, &mut files);
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

fn collect(directory: &Path, files: &mut Vec<(String, String)>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("no se puede leer {}: {error}", directory.display()));

    for entry in entries {
        let path = entry.expect("entrada de directorio ilegible").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let contents = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            files.push((relative(&path), contents));
        }
    }
}

/// Ruta desde la raíz del crate, con `/` en cualquier plataforma: es la forma
/// en que las listas de arriba se leen y se editan a mano.
fn relative(path: &Path) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.strip_prefix(&root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------
// Las reglas, como funciones puras sobre el texto de un archivo.
//
// Separadas del recorrido del disco a propósito: un test que afirma *ausencia*
// —"nadie lee hardware", "presentación no nombra la simulación"— da la misma
// salida verde si la ley se cumple que si el detector está ciego. La única
// forma de distinguirlas es plantarle una infracción y exigir que la vea, y
// para eso la regla tiene que poder ejecutarse sobre texto inventado.
//
// No es hipotético: el corte entre producción y tests estuvo mal durante meses
// y dejó medio `world/forest.rs` invisible para dos de estas reglas.
// ---------------------------------------------------------------------------

fn reads_hardware(contents: &str) -> bool {
    let source = production_source(contents);
    HARDWARE.iter().any(|symbol| source.contains(symbol))
}

fn has_unsafe(contents: &str) -> bool {
    let source = production_source(contents);
    source.contains("unsafe ") || source.contains("unsafe{")
}

fn reaches_facade_or_physics(source: &str) -> bool {
    source.contains("use bevy::") || source.contains("avian3d")
}

fn reaches_rendering(source: &str) -> bool {
    source.contains("use bevy::") || source.contains("Mesh") || source.contains("StandardMaterial")
}

fn installs_an_inner_simulation_plugin(contents: &str) -> bool {
    production_source(contents)
        .split("add_plugins")
        .skip(1)
        // Sólo los argumentos de la llamada, hasta el cierre del statement: sin
        // este corte el `split` se lleva el resto del archivo y cualquier
        // mención posterior al crate es un falso positivo (le pasó a `scene`,
        // que nombra `spawn_player` cien líneas más abajo).
        .filter_map(|rest| rest.split_once(");").map(|(argument, _)| argument))
        .any(|argument| {
            // `bof_simulation::SimulationPlugin` es el único permitido, y sólo
            // lo agrega `main`. Cualquier otra ruta del crate es un plugin
            // interno que `SimulationPlugin` ya instaló.
            argument
                .split("bof_simulation::")
                .skip(1)
                .any(|tail| !tail.starts_with("SimulationPlugin"))
        })
}

/// Materiales que entran al render por la puerta de atrás.
///
/// Regla pura sobre texto para que el canario pueda plantarle una infracción: un
/// test que afirma *ausencia* da verde igual si la ley se cumple que si el
/// detector está ciego.
fn materials_registered_outside_the_registry(files: &[(String, String)]) -> Vec<String> {
    // El material del propio diagnóstico de overdraw: instrumentarlo sería
    // pedirle que se reemplace a sí mismo, y el inventario se apaga entero
    // mientras esa vista corre porque su escena no es una muestra válida.
    const EXEMPT: &[&str] = &["AdditiveOverdrawMaterial"];
    const REGISTRY: &str = "src/visuals/material_registry.rs";
    let mut escapes = Vec::new();
    for (path, contents) in files {
        if path == REGISTRY {
            continue; // Es el único lugar donde `MaterialPlugin` se instala.
        }
        for tail in contents.split("MaterialPlugin::<").skip(1) {
            let Some((material, _)) = tail.split_once('>') else {
                continue;
            };
            let material = material.trim();
            if EXEMPT.contains(&material) {
                continue;
            }
            escapes.push(format!("{material} (registrado en {path})"));
        }
    }
    escapes
}

fn names_the_simulation_crate(contents: &str) -> bool {
    contents.contains("bof_simulation")
}

/// C2: el hardware se muestrea una vez, en `input`, y todos los demás consumen
/// acciones tipadas. Mientras la deuda exista, al menos no crece.
#[test]
fn only_the_input_module_reads_hardware() {
    let mut offenders = Vec::new();

    for (path, contents) in source_files() {
        if path.starts_with("src/input/") {
            continue;
        }
        if reads_hardware(&contents) {
            offenders.push(path);
        }
    }

    let new: Vec<&String> = offenders
        .iter()
        .filter(|path| !HARDWARE_DEBT.contains(&path.as_str()))
        .collect();
    assert!(
        new.is_empty(),
        "leen hardware fuera de `src/input/` y no estaban en la deuda conocida \
         (C2, `docs/ARCHITECTURE.md`): {new:?}. Consumí `ActiveActions` en vez \
         de `ButtonInput`; si de verdad hace falta el hardware crudo, la \
         excepción se discute antes de agregarse a `HARDWARE_DEBT`."
    );

    let paid: Vec<&&str> = HARDWARE_DEBT
        .iter()
        .filter(|path| !offenders.iter().any(|offender| offender == *path))
        .collect();
    assert!(
        paid.is_empty(),
        "ya no leen hardware pero siguen en `HARDWARE_DEBT`: {paid:?}. \
         Borrá esas líneas — la lista solo sirve si encoge."
    );
}

/// §12: sin `unsafe` en el proyecto. Hoy se cumple; esto lo mantiene así hasta
/// que `[lints.rust] unsafe_code = "forbid"` lo cobre en el build (fase 3).
#[test]
fn the_project_has_no_unsafe_code() {
    let offenders: Vec<String> = all_source_files()
        .into_iter()
        .filter(|(_, contents)| has_unsafe(contents))
        .map(|(path, _)| path)
        .collect();

    assert!(
        offenders.is_empty(),
        "§12 dice que este proyecto no lleva `unsafe`: {offenders:?}"
    );
}

/// Phase 5: domain is a compile-time data boundary, not a smaller alias for
/// the full engine facade. Rendering and physics must remain unreachable from
/// it so both sibling crates can depend on the same contracts without gaining
/// access to each other's implementation details.
#[test]
fn domain_has_no_render_or_physics_dependencies() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("crates/domain/Cargo.toml"))
        .expect("domain manifest must be readable");
    for forbidden in ["bevy =", "avian3d", "bevy_render", "bevy_pbr"] {
        assert!(
            !manifest.contains(forbidden),
            "bof_domain must not declare {forbidden:?}"
        );
    }

    let mut files = Vec::new();
    collect(&root.join("crates/domain/src"), &mut files);
    let offenders: Vec<_> = files
        .into_iter()
        .filter(|(_, source)| reaches_facade_or_physics(source))
        .map(|(path, _)| path)
        .collect();
    assert!(
        offenders.is_empty(),
        "bof_domain reached the Bevy facade or Avian: {offenders:?}"
    );
}

/// Phase 6.1: simulation owns physics but must remain runnable without a
/// window, renderer, or the full Bevy facade (§20).
#[test]
fn simulation_has_no_render_dependencies() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("crates/simulation/Cargo.toml"))
        .expect("simulation manifest must be readable");
    for forbidden in ["bevy =", "bevy_render", "bevy_pbr"] {
        assert!(
            !manifest.contains(forbidden),
            "bof_simulation must not declare {forbidden:?}"
        );
    }

    let mut files = Vec::new();
    collect(&root.join("crates/simulation/src"), &mut files);
    let offenders: Vec<_> = files
        .into_iter()
        .filter(|(_, source)| reaches_rendering(source))
        .map(|(path, _)| path)
        .collect();
    assert!(
        offenders.is_empty(),
        "bof_simulation reached rendering or the Bevy facade: {offenders:?}"
    );
}

/// Desde la fase 6.8, `bof_simulation` arma su propio grafo: `SimulationPlugin`
/// registra los once plugins de gameplay y el orden entre ellos. La app agrega
/// **ese** plugin y ninguno de los de adentro.
///
/// Existe porque el compilador no ayuda acá: registrar un plugin dos veces
/// compila igual y revienta al arrancar (`plugin was already added`). Pasó el
/// 2026-08-04 con `world::WorldPlugin`, que la app seguía instalando dentro del
/// suyo; ningún test lo vio porque ninguno arma la app real, y hace falta una
/// ventana para armarla.
#[test]
fn the_app_installs_simulation_only_through_its_root_plugin() {
    let offenders: Vec<String> = source_files()
        .into_iter()
        .filter(|(path, _)| path != "src/main.rs")
        .filter(|(_, contents)| installs_an_inner_simulation_plugin(contents))
        .map(|(path, _)| path)
        .collect();

    assert!(
        offenders.is_empty(),
        "estos archivos instalan un plugin que `SimulationPlugin` ya registra, \
         y Bevy panickea al arrancar por plugin duplicado: {offenders:#?}"
    );
}

/// Los módulos de presentación: dibujan, oyen y muestran, pero no simulan.
const PRESENTATION: &[&str] = &[
    "src/camera/",
    "src/debug/",
    "src/presentation/",
    "src/sfx/",
    "src/visuals/",
];

/// §20 con el grafo de crates hermanas: presentación sólo ve `bof_domain`, que
/// es dato puro, así que **leer es lo único que puede hacer**.
///
/// Mientras `bof_presentation` no sea un crate propio, Cargo no lo cobra: los
/// módulos viven en el binario, que sí ve la simulación. Este test ocupa ese
/// lugar. Medido el 2026-08-04, presentación ya cumplía la ley a mano — cero
/// `&mut` sobre componentes de simulación, todo pedido por mensaje — y el corte
/// de la fase 7 dejó la lista en cero. Sólo puede seguir en cero.
#[test]
fn presentation_never_names_the_simulation_crate() {
    let offenders: Vec<String> = source_files()
        .into_iter()
        .filter(|(path, _)| PRESENTATION.iter().any(|prefix| path.starts_with(prefix)))
        .filter(|(_, contents)| names_the_simulation_crate(contents))
        .map(|(path, _)| path)
        .collect();

    assert!(
        offenders.is_empty(),
        "presentación sólo puede leer `bof_domain`; si necesita algo de \
         simulación, ese algo es dato y baja a domain, o se pide por mensaje: \
         {offenders:#?}"
    );
}

/// Los canarios: cada regla enfrentada a una infracción inventada y a su
/// contraparte legítima.
///
/// Sin esto, las seis leyes de arriba pueden estar vigilando la nada y se ven
/// exactamente igual de verdes. Un test que nunca se vio fallar no es un test.
/// Los falsos negativos que este archivo ya tuvo —el corte de `#[cfg(test)]`
/// que cegaba medio archivo, el `split` que se llevaba el resto del texto—
/// habrían muerto acá el día que se escribieron.
mod canaries {
    use super::*;

    /// La ley del presupuesto nació de una omisión que duró meses; el canario
    /// existe para que no vuelva a nacer ciega — y una vez ya nació ciega,
    /// cuando el conteo se volvió genérico y la ley se quedó sin nada que
    /// buscar.
    #[test]
    fn the_registry_rule_sees_a_material_that_skips_it() {
        let planted = vec![(
            "src/visuals/water.rs".to_string(),
            "app.add_plugins(MaterialPlugin::<WaterMaterial>::default());".to_string(),
        )];
        assert_eq!(
            materials_registered_outside_the_registry(&planted),
            vec!["WaterMaterial (registrado en src/visuals/water.rs)".to_string()],
        );
        let through_the_registry = vec![(
            "src/visuals/water.rs".to_string(),
            "app.add_instrumented_material::<WaterMaterial>();".to_string(),
        )];
        assert!(
            materials_registered_outside_the_registry(&through_the_registry).is_empty(),
            "un material que sí pasa por el registro no puede reportarse como fuga"
        );
    }

    #[test]
    fn the_hardware_rule_sees_a_planted_reader() {
        assert!(
            reads_hardware("fn s(keys: Res<ButtonInput<KeyCode>>) {}"),
            "la regla de hardware no ve un `ButtonInput` en producción"
        );
        assert!(
            !reads_hardware("#[cfg(test)]\nmod tests {\n  Res<ButtonInput<KeyCode>>\n}"),
            "los tests sí pueden fabricar input: simular una tecla es la única \
             forma de probar un sistema que la consume"
        );
        assert!(
            reads_hardware("fn real(k: Res<ButtonInput<KeyCode>>) {}\n#[cfg(test)]\nmod tests {}"),
            "un `mod tests` al final no puede esconder lo que hay antes"
        );
        // El falso negativo real, encontrado el 2026-08-04: `world/forest.rs`
        // tiene un `#[cfg(test)]` sobre una función a media altura, y el corte
        // anterior —el primer `#[cfg(test)]` del archivo— dejaba ciega a la
        // regla de ahí para abajo. Un atributo suelto arriba alcanzaba para
        // apagar la ley entera.
        assert!(
            reads_hardware(
                "#[cfg(test)]\nfn fixture() {}\nfn real(k: Res<ButtonInput<KeyCode>>) {}"
            ),
            "un `#[cfg(test)]` sobre una función suelta no puede cegar lo que sigue"
        );
    }

    #[test]
    fn the_unsafe_rule_sees_a_planted_block() {
        assert!(has_unsafe("unsafe { *ptr }"), "no ve un bloque `unsafe`");
        assert!(
            !has_unsafe("#[cfg(test)]\nmod tests {\n unsafe { }\n}"),
            "sólo mira producción"
        );
        assert!(
            !has_unsafe("// nada inseguro acá\nfn safe() {}"),
            "no puede acusar a código limpio"
        );
    }

    #[test]
    fn the_domain_rule_sees_the_facade_and_physics() {
        assert!(reaches_facade_or_physics("use bevy::prelude::*;"));
        assert!(reaches_facade_or_physics("use avian3d::prelude::Collider;"));
        assert!(
            !reaches_facade_or_physics("use bevy_ecs::prelude::*;\nuse bevy_math::Vec3;"),
            "los crates sueltos de Bevy son justo lo que domain sí puede usar"
        );
    }

    #[test]
    fn the_simulation_rule_sees_rendering() {
        assert!(reaches_rendering("Mesh3d(meshes.add(shape))"));
        assert!(reaches_rendering(
            "MeshMaterial3d(materials.add(StandardMaterial"
        ));
        assert!(
            !reaches_rendering("use avian3d::prelude::*;\nuse bevy_ecs::prelude::*;"),
            "la física sí es de simulación; lo prohibido es el render"
        );
    }

    #[test]
    fn the_plugin_rule_sees_an_inner_plugin() {
        assert!(
            installs_an_inner_simulation_plugin(
                "app.add_plugins(bof_simulation::world::WorldPlugin);"
            ),
            "no ve el doble registro que rompió el arranque el 2026-08-04"
        );
        assert!(
            !installs_an_inner_simulation_plugin("app.add_plugins(SimulationPlugin);"),
            "el plugin raíz es justamente el que sí se instala"
        );
        assert!(
            !installs_an_inner_simulation_plugin(
                "app.add_plugins(menu::MenuPlugin);\nfn later() { bof_simulation::player::spawn_player(); }"
            ),
            "mencionar el crate lejos de un `add_plugins` no es instalar un plugin"
        );
    }

    #[test]
    fn the_presentation_rule_sees_the_simulation_crate() {
        assert!(names_the_simulation_crate(
            "use bof_simulation::movement::Actor;"
        ));
        assert!(
            !names_the_simulation_crate("use bof_domain::movement::Actor;"),
            "leer domain es exactamente lo que presentación sí puede hacer"
        );
    }
}

/// §15 con dientes. La ley decía "comentarios solo para invariantes" desde
/// siempre y no alcanzó: el 2026-08-06 `visuals/grass.rs` llegó a 46% de
/// comentario — 637 líneas sobre 1371 — y cada bloque era defendible por
/// separado. Lo que ninguno de ellos podía ver es el total.
///
/// El techo se cobra por archivo y no por proyecto a propósito: un promedio
/// sano esconde exactamente el caso que hubo, un módulo enterrado en prosa
/// mientras el resto respira.
#[test]
fn no_file_is_mostly_commentary() {
    const CEILING_PERCENT: usize = 30;
    // Debajo de esto el porcentaje es ruido: en un archivo de 40 líneas, doce
    // de comentario ya lo pasan y no hay nada que arreglar.
    const MINIMUM_LINES: usize = 100;

    let offenders: Vec<String> = all_source_files()
        .into_iter()
        .filter_map(|(path, contents)| {
            let lines: Vec<&str> = contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect();
            if lines.len() < MINIMUM_LINES {
                return None;
            }
            let commented = lines.iter().filter(|line| line.starts_with("//")).count();
            let over = commented * 100 > CEILING_PERCENT * lines.len();
            over.then(|| {
                let percent = commented as f64 * 100.0 / lines.len() as f64;
                format!("{path} {percent:.2}% ({commented}/{})", lines.len())
            })
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "§15 pone el techo en {CEILING_PERCENT}% de comentario por archivo; \
         el rationale largo va a docs/: {offenders:#?}"
    );
}

/// Un material se registra por el registro, o no se registra.
///
/// La ausencia de esta ley costó meses: `GrassMaterial` existía desde que existe
/// la pradera y `collect_scene` sólo consultaba `StandardMaterial` y
/// `TerrainMaterial`, así que el grader calificaba la escena sin lo más caro que
/// hay en ella — medido el 2026-08-06 en la caja Pasto: 33.792 triángulos
/// declarados con cien mil briznas en pantalla.
///
/// **La versión anterior de esta ley pedía que el material apareciera nombrado
/// en `collect_scene`**, y eso dejó de significar nada el día en que el conteo
/// se volvió genérico: sin tipos escritos ahí, la ley pasaba sola. Una ley que
/// no puede fallar es una ley apagada. La forma que sí se sostiene es prohibir
/// la puerta de atrás: `MaterialPlugin::<M>` sólo se instala dentro del
/// registro, y el resto del código pide `add_instrumented_material::<M>()` —
/// que engancha el inventario y el overdraw en el mismo acto.
#[test]
fn no_material_is_registered_outside_the_registry() {
    let escapes = materials_registered_outside_the_registry(&source_files());
    assert!(
        escapes.is_empty(),
        "estos materiales entran al render sin sus herramientas; usar \
         `add_instrumented_material::<M>()` o eximirlos con su razón: {escapes:#?}"
    );
}

/// El editor y el juego son **contextos distintos**: en modo juego
/// `EditorPlugin` no se instala, así que no existen sus recursos, su HUD ni F5.
///
/// Se verifica sobre el texto de `main.rs` porque armar la app real pide una
/// ventana, igual que el test de arriba. Lo que se busca es que el único
/// `add_plugins` del editor viva detrás de la comparación con el modo: un
/// `add_plugins(editor::EditorPlugin)` suelto —que es como estaba antes del
/// 2026-08-22, y como quedaría al "limpiar" el `if`— vuelve a meter la
/// herramienta de autoría dentro del juego sin que nada avise.
#[test]
fn the_editor_is_installed_only_in_editor_mode() {
    let main = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("main.rs must be readable");
    let production = production_source(&main);

    let installs: Vec<&str> = production
        .lines()
        .filter(|line| line.contains("editor::EditorPlugin"))
        .collect();
    assert_eq!(
        installs.len(),
        1,
        "`editor::EditorPlugin` tiene que instalarse en un solo sitio: {installs:#?}"
    );
    assert!(
        production.contains("if mode == scene::AppMode::Editor"),
        "`main` dejó de separar los contextos: el editor se instala sin \
         preguntar por el modo, o sea que el juego volvió a tener F5"
    );
}
