//! Leyes de `ARCHITECTURE.md` que el compilador todavía no puede cobrar.
//!
//! C2 (solo `input` lee hardware) y §12 (sin `unsafe`) son verificables por
//! máquina, y hasta hoy no las verificaba nadie: C2 se documentó el 2026-07-25
//! con 14 archivos, llegó a 15 el 2026-08-01 y desde entonces empezó a bajar.
//! Una ley que solo vive en prosa no es una ley.
//!
//! Esto es andamiaje deliberado: cuando `src/input/` sea el único crate que
//! declare `bevy_input` (fase 6 de `docs/CRATES.md`), `KeyCode` dejará de ser
//! nombrable fuera de él y este archivo se borra. Mientras tanto **congela la
//! deuda**: `HARDWARE_DEBT` solo puede encoger, y el test falla tanto si
//! aparece un infractor nuevo como si un archivo de la lista deja de serlo sin
//! sacarlo de ella.

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
    "src/editor/mod.rs",
    "src/editor/paint.rs",
    "src/editor/persist.rs",
    "src/presentation/debug_ui/hud_menu.rs",
    "src/presentation/debug_ui/mod.rs",
    "src/presentation/inventory_ui/mod.rs",
    "src/scene/menu.rs",
    "src/scene/mod.rs",
    "src/visuals/grass.rs",
];

/// Los tests sí pueden fabricar input: simular una tecla es la única forma de
/// probar un sistema que la consume. El corte es el primer `#[cfg(test)]` del
/// archivo, que es donde este repo pone sus módulos de test.
fn production_source(contents: &str) -> &str {
    match contents.find("#[cfg(test)]") {
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

/// C2: el hardware se muestrea una vez, en `input`, y todos los demás consumen
/// acciones tipadas. Mientras la deuda exista, al menos no crece.
#[test]
fn only_the_input_module_reads_hardware() {
    let mut offenders = Vec::new();

    for (path, contents) in source_files() {
        if path.starts_with("src/input/") {
            continue;
        }
        let source = production_source(&contents);
        if HARDWARE.iter().any(|symbol| source.contains(symbol)) {
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
    let offenders: Vec<String> = source_files()
        .into_iter()
        .filter(|(_, contents)| {
            let source = production_source(contents);
            source.contains("unsafe ") || source.contains("unsafe{")
        })
        .map(|(path, _)| path)
        .collect();

    assert!(
        offenders.is_empty(),
        "§12 dice que este proyecto no lleva `unsafe`: {offenders:?}"
    );
}
