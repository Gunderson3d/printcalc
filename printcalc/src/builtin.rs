//! Built-in printer/filament presets, embedded at compile time from
//! `builtin/*.toml` (via `include_str!`) — plain data baked into the
//! binary, no filesystem involved at runtime, so these are available to
//! `printcalc-wasm` too. [`crate::calculate`] (and anything that
//! resolves a `Linked` ref through it) falls back to these when a name
//! isn't an embedded profile; the CLI additionally checks its on-disk
//! profile library first.
//!
//! `printers.toml`/`filaments.toml`/`resin.toml` hold vendor-neutral
//! starter data and are always compiled in. `bambu_printers.toml`/
//! `bambu_filaments.toml` and `prusa_printers.toml`/`prusa_filaments.toml`
//! hold real vendor catalogs pulled from each slicer's own system
//! profiles, gated behind the `bambu`/`prusa` Cargo features (same
//! features [`crate::formats`] gates its parsers behind) — a build with
//! only one vendor feature enabled only bakes in that vendor's presets.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::model::filament::MaterialProfile;
use crate::model::printer::PrinterProfile;

#[derive(Deserialize)]
struct PrintersFile {
    printer: Vec<PrinterProfile>,
}

#[derive(Deserialize)]
struct FilamentsFile {
    material: Vec<MaterialProfile>,
}

fn parse_printers(toml_str: &str) -> Vec<PrinterProfile> {
    toml::from_str::<PrintersFile>(toml_str)
        .expect("builtin printer TOML is well-formed")
        .printer
}

fn parse_filaments(toml_str: &str) -> Vec<MaterialProfile> {
    toml::from_str::<FilamentsFile>(toml_str)
        .expect("builtin filament TOML is well-formed")
        .material
}

#[cfg(feature = "bambu")]
fn bambu_printers() -> Vec<PrinterProfile> {
    parse_printers(include_str!("../builtin/bambu_printers.toml"))
}
#[cfg(not(feature = "bambu"))]
fn bambu_printers() -> Vec<PrinterProfile> {
    Vec::new()
}

#[cfg(feature = "prusa")]
fn prusa_printers() -> Vec<PrinterProfile> {
    parse_printers(include_str!("../builtin/prusa_printers.toml"))
}
#[cfg(not(feature = "prusa"))]
fn prusa_printers() -> Vec<PrinterProfile> {
    Vec::new()
}

#[cfg(feature = "bambu")]
fn bambu_filaments() -> Vec<MaterialProfile> {
    parse_filaments(include_str!("../builtin/bambu_filaments.toml"))
}
#[cfg(not(feature = "bambu"))]
fn bambu_filaments() -> Vec<MaterialProfile> {
    Vec::new()
}

#[cfg(feature = "prusa")]
fn prusa_filaments() -> Vec<MaterialProfile> {
    parse_filaments(include_str!("../builtin/prusa_filaments.toml"))
}
#[cfg(not(feature = "prusa"))]
fn prusa_filaments() -> Vec<MaterialProfile> {
    Vec::new()
}

static PRINTERS: LazyLock<Vec<PrinterProfile>> = LazyLock::new(|| {
    let mut printers = parse_printers(include_str!("../builtin/printers.toml"));
    printers.extend(bambu_printers());
    printers.extend(prusa_printers());
    printers
});

static FILAMENTS: LazyLock<Vec<MaterialProfile>> = LazyLock::new(|| {
    let mut filaments = parse_filaments(include_str!("../builtin/filaments.toml"));
    filaments.extend(parse_filaments(include_str!("../builtin/resin.toml")));
    filaments.extend(bambu_filaments());
    filaments.extend(prusa_filaments());
    filaments
});

/// Every built-in printer preset.
pub fn printers() -> &'static [PrinterProfile] {
    &PRINTERS
}

/// A built-in printer preset by name.
pub fn printer(name: &str) -> Option<&'static PrinterProfile> {
    PRINTERS.iter().find(|p| p.name == name)
}

/// Every built-in filament preset (filament and resin kinds alike).
pub fn filaments() -> &'static [MaterialProfile] {
    &FILAMENTS
}

/// A built-in filament preset by name.
pub fn filament(name: &str) -> Option<&'static MaterialProfile> {
    FILAMENTS.iter().find(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_resolve_and_unknown_names_dont() {
        #[cfg(feature = "bambu")]
        assert_eq!(
            printer("bambu-a1").unwrap().kind,
            crate::model::printer::PrinterKind::Fdm
        );
        assert!(printer("nonexistent").is_none());
        assert_eq!(
            filament("pla-black").unwrap().kind,
            crate::model::filament::MaterialKind::Filament
        );
        assert_eq!(
            filament("generic-resin-grey").unwrap().kind,
            crate::model::filament::MaterialKind::Resin
        );
        assert!(filament("nonexistent").is_none());
    }

    #[cfg(all(feature = "bambu", not(feature = "prusa")))]
    #[test]
    fn bambu_only_feature_excludes_prusa_builtins() {
        assert!(printer("bambu-x1-carbon").is_some());
        assert!(printer("prusa-mk4s").is_none());
        assert!(filament("bambu-pla-basic").is_some());
        assert!(filament("prusament-pla").is_none());
    }

    #[cfg(all(feature = "prusa", not(feature = "bambu")))]
    #[test]
    fn prusa_only_feature_excludes_bambu_builtins() {
        assert!(printer("prusa-mk4s").is_some());
        assert!(printer("bambu-x1-carbon").is_none());
        assert!(filament("prusament-pla").is_some());
        assert!(filament("bambu-pla-basic").is_none());
    }
}
