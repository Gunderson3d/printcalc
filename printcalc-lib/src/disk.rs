//! Profile library load/save (`~/.config/printcalc/{printers,filaments}/*.toml`)
//! and global config (`~/.config/printcalc/config.toml`).
//!
//! Filesystem-only — not available to `printcalc-wasm`, which has no
//! filesystem and instead receives embedded profiles directly.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::filament::FilamentProfile;
use crate::model::printer::PrinterProfile;

/// Why a profile library or global config operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum LibraryError {
    /// A read/write against the profile library failed.
    Io(std::io::Error),
    /// TOML (de)serialization failed; the message is `toml`'s own error text.
    Toml(String),
    /// No profile saved under this name.
    NotFound {
        /// `"printer"` or `"filament"`.
        kind: &'static str,
        /// The name that was looked up.
        name: String,
    },
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Io(e) => write!(f, "io error: {e}"),
            LibraryError::Toml(e) => write!(f, "toml error: {e}"),
            LibraryError::NotFound { kind, name } => write!(f, "{kind} {name:?} not found"),
        }
    }
}

impl std::error::Error for LibraryError {}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::Io(e)
    }
}

/// `~/.config/printcalc/config.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Fallback for `Job::electricity_cost_per_kwh_override`.
    pub electricity_cost_per_kwh: f64,
    /// Fallback for `LaborCost::rate_per_hour`.
    pub labor_rate_per_hour: f64,
    /// Fallback for `Job::markup_percentages_override`. Default `[50.0,
    /// 60.0, 70.0]`.
    pub markup_percentages: Vec<f64>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        GlobalConfig {
            electricity_cost_per_kwh: 0.14,
            labor_rate_per_hour: 15.0,
            markup_percentages: vec![50.0, 60.0, 70.0],
        }
    }
}

/// `~/.config/printcalc/` (via the `dirs` crate), or `$PRINTCALC_CONFIG_DIR`
/// if set — an override real users can reach for (a second profile set, a
/// portable/synced location) and tests use to avoid touching the real
/// user config.
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PRINTCALC_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("printcalc")
}

/// `<config_dir>/printers/`.
pub fn printers_dir() -> PathBuf {
    config_dir().join("printers")
}

/// `<config_dir>/filaments/`.
pub fn filaments_dir() -> PathBuf {
    config_dir().join("filaments")
}

/// Falls back to [`GlobalConfig::default`] if `config.toml` can't be read
/// — not just missing, *any* read failure. There's no `printcalc config
/// init` step, so a fresh install (or a platform with no real filesystem
/// at all, like `printcalc-wasm` in a browser) just works with the
/// defaults instead of hard-failing every calc.
pub fn load_global_config() -> Result<GlobalConfig, LibraryError> {
    let path = config_dir().join("config.toml");
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| LibraryError::Toml(e.to_string())),
        Err(_) => Ok(GlobalConfig::default()),
    }
}

fn save_profile<T: Serialize>(dir: &Path, name: &str, profile: &T) -> Result<(), LibraryError> {
    fs::create_dir_all(dir)?;
    let text = toml::to_string(profile).map_err(|e| LibraryError::Toml(e.to_string()))?;
    fs::write(dir.join(format!("{name}.toml")), text)?;
    Ok(())
}

fn load_profile<T: DeserializeOwned>(
    dir: &Path,
    kind: &'static str,
    name: &str,
) -> Result<T, LibraryError> {
    let path = dir.join(format!("{name}.toml"));
    let text = fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LibraryError::NotFound {
                kind,
                name: name.to_string(),
            }
        } else {
            LibraryError::Io(e)
        }
    })?;
    toml::from_str(&text).map_err(|e| LibraryError::Toml(e.to_string()))
}

fn list_profiles(dir: &Path) -> Result<Vec<String>, LibraryError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    names.sort();
    Ok(names)
}

/// Saves `profile` to `printers_dir()/<name>.toml`, overwriting any
/// existing file under that name.
pub fn save_printer(profile: &PrinterProfile) -> Result<(), LibraryError> {
    save_profile(&printers_dir(), &profile.name, profile)
}

/// Loads a saved printer profile by name.
pub fn load_printer(name: &str) -> Result<PrinterProfile, LibraryError> {
    load_profile(&printers_dir(), "printer", name)
}

/// Every saved printer profile's name, sorted. Empty (not an error) if
/// `printers_dir()` doesn't exist yet.
pub fn list_printers() -> Result<Vec<String>, LibraryError> {
    list_profiles(&printers_dir())
}

/// Saves `profile` to `filaments_dir()/<name>.toml`, overwriting any
/// existing file under that name.
pub fn save_filament(profile: &FilamentProfile) -> Result<(), LibraryError> {
    save_profile(&filaments_dir(), &profile.name, profile)
}

/// Loads a saved filament profile by name.
pub fn load_filament(name: &str) -> Result<FilamentProfile, LibraryError> {
    load_profile(&filaments_dir(), "filament", name)
}

/// Every saved filament profile's name, sorted. Empty (not an error) if
/// `filaments_dir()` doesn't exist yet.
pub fn list_filaments() -> Result<Vec<String>, LibraryError> {
    list_profiles(&filaments_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::filament::MaterialKind;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("printcalc-lib-test-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn save_load_and_list_round_trip() {
        let dir = temp_dir("save-load-list");
        let profile = FilamentProfile {
            name: "pla-black".into(),
            kind: MaterialKind::Filament,
            price_per_kg: Some(18.99),
            price_per_liter: None,
            density_g_per_cm3: None,
            efficiency_factor: 1.1,
            color: Some("black".into()),
            vendor: None,
        };

        save_profile(&dir, &profile.name, &profile).unwrap();
        let loaded: FilamentProfile = load_profile(&dir, "filament", "pla-black").unwrap();
        assert_eq!(loaded, profile);
        assert_eq!(list_profiles(&dir).unwrap(), vec!["pla-black".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_profile_is_not_found() {
        let dir = temp_dir("missing");
        let err = load_profile::<FilamentProfile>(&dir, "filament", "nope").unwrap_err();
        assert!(matches!(
            err,
            LibraryError::NotFound { kind: "filament", name } if name == "nope"
        ));
    }

    #[test]
    fn list_of_missing_dir_is_empty() {
        let dir = temp_dir("list-missing-dir");
        assert_eq!(list_profiles(&dir).unwrap(), Vec::<String>::new());
    }
}
