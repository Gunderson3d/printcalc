//! `printcalc filament` — add, edit, remove, list and inspect saved
//! filament and resin profiles.
//!
//! One command covers both: `--kind` decides whether `--price` is read
//! per kilogram (filament, measured in grams) or per litre (resin,
//! measured in millilitres). As with printers, only a name is required
//! for `add`, and `edit` keeps whatever isn't passed.
use std::error::Error;

use clap::{Args, Subcommand};

use libprintcalc::model::filament::{MaterialKind, MaterialProfile};

use crate::interactive;
use crate::ops;

#[derive(Args)]
pub struct FilamentArgs {
    #[command(subcommand)]
    pub command: FilamentCommand,
}

/// The settable fields, shared by `add` and `edit`. Every one is
/// optional at the flag level; what an omitted one means depends on the
/// command (the documented default for `add`, the saved value for
/// `edit`).
#[derive(Args)]
pub struct FilamentFields {
    /// "filament" or "resin". [default: filament]
    #[arg(short, long)]
    kind: Option<String>,
    /// Price per kg (filament) or per liter (resin). [default: 20]
    #[arg(long)]
    price: Option<f64>,
    /// Waste allowance applied before pricing: how much more material
    /// you're charged for than the slicer predicted, covering purge,
    /// skirts and failed prints. 1.5 prices 50% extra. [default: 1.5]
    #[arg(long = "efficiency-factor")]
    efficiency_factor: Option<f64>,
    /// Free-form display color — a name or a hex code. Does not
    /// affect cost.
    #[arg(long)]
    color: Option<String>,
    /// Manufacturer. Does not affect cost, but auto-detection rejects
    /// a match whose stated vendor disagrees with this one.
    #[arg(long)]
    vendor: Option<String>,
}

#[derive(Subcommand)]
pub enum FilamentCommand {
    /// Save a filament or resin profile. Only `--name` is required;
    /// `--price` and `--efficiency-factor` fall back to their documented
    /// defaults (see `docs/reference/pricing.md`).
    Add {
        /// Profile name. Also the filename it's saved under, and what
        /// `--filament` looks up.
        ///
        /// Auto-detection matches this against the name the slicer
        /// recorded, so naming a profile after the slicer's preset (e.g.
        /// `bambu-pla-basic`) makes it match exactly.
        #[arg(short, long)]
        name: Option<String>,
        #[command(flatten)]
        fields: FilamentFields,
    },
    /// Change a saved filament profile. Flags set fields; anything not
    /// passed keeps its saved value (interactively, price and efficiency
    /// factor prompt with the saved value shown and a blank answer keeps
    /// it). Editing a built-in preset saves a copy under the same name
    /// that shadows it.
    Edit {
        /// Profile name, as shown by `printcalc filament list`.
        name: String,
        #[command(flatten)]
        fields: FilamentFields,
    },
    /// Delete a saved filament profile. Built-in presets can't be
    /// removed; a saved profile shadowing one just stops shadowing it.
    #[command(visible_alias = "rm")]
    Remove {
        /// Profile name, as shown by `printcalc filament list`.
        name: String,
    },
    /// List every saved filament profile by name.
    List,
    /// Print a saved filament profile's values.
    Show {
        /// Profile name, as shown by `printcalc filament list`.
        name: String,
    },
}

pub fn run(cmd: FilamentCommand, interactive_mode: bool) -> Result<(), Box<dyn Error>> {
    match cmd {
        FilamentCommand::Add { name, fields } => {
            let name =
                interactive::require_string(name, "name", "Filament name", interactive_mode)?;
            let kind = parse_kind(fields.kind.as_deref())?.unwrap_or(MaterialKind::Filament);
            let base = MaterialProfile::new(name.clone(), kind);
            let profile = resolve_profile(&fields, base, interactive_mode)?;
            ops::add_filament(&profile)?;
            println!("saved filament {name:?}");
            Ok(())
        }
        FilamentCommand::Edit { name, fields } => {
            let mut current = ops::show_filament(&name)?;
            let shadows_builtin = !ops::list_saved_filaments()?.contains(&name);
            if let Some(kind) = parse_kind(fields.kind.as_deref())? {
                current.kind = kind;
            }
            let profile = resolve_profile(&fields, current, interactive_mode)?;
            ops::add_filament(&profile)?;
            if shadows_builtin {
                println!("saved filament {name:?} (shadows the built-in preset)");
            } else {
                println!("updated filament {name:?}");
            }
            Ok(())
        }
        FilamentCommand::Remove { name } => {
            ops::remove_filament(&name)?;
            println!("removed filament {name:?}");
            Ok(())
        }
        FilamentCommand::List => {
            for name in ops::list_filaments()? {
                println!("{name}");
            }
            Ok(())
        }
        FilamentCommand::Show { name } => {
            let profile = ops::show_filament(&name)?;
            print_filament(&profile);
            Ok(())
        }
    }
}

fn parse_kind(kind: Option<&str>) -> Result<Option<MaterialKind>, Box<dyn Error>> {
    match kind {
        None => Ok(None),
        Some("filament") => Ok(Some(MaterialKind::Filament)),
        Some("resin") => Ok(Some(MaterialKind::Resin)),
        Some(other) => {
            Err(format!("invalid --kind {other:?} (expected \"filament\" or \"resin\")").into())
        }
    }
}

/// `base` with every field `fields` sets replaced. Price and efficiency
/// factor prompt (with `base`'s value shown) when interactive; color and
/// vendor are flag-only, since blank is a legitimate value for them.
fn resolve_profile(
    fields: &FilamentFields,
    mut base: MaterialProfile,
    interactive_mode: bool,
) -> Result<MaterialProfile, Box<dyn Error>> {
    base.price = interactive::defaulted_f64(
        fields.price,
        &format!("Price per {}", base.kind.price_unit()),
        base.price,
        interactive_mode,
    )?;
    base.efficiency_factor = interactive::defaulted_f64(
        fields.efficiency_factor,
        "Efficiency factor",
        base.efficiency_factor,
        interactive_mode,
    )?;
    if fields.color.is_some() {
        base.color = fields.color.clone();
    }
    if fields.vendor.is_some() {
        base.vendor = fields.vendor.clone();
    }
    Ok(base)
}

fn print_filament(f: &MaterialProfile) {
    println!("name: {}", f.name);
    println!("kind: {:?}", f.kind);
    let price_unit = f.kind.price_unit();
    println!("price: {} per {price_unit}", f.price);
    println!("efficiency_factor: {}", f.efficiency_factor);
    if let Some(c) = &f.color {
        println!("color: {c}");
    }
    if let Some(v) = &f.vendor {
        println!("vendor: {v}");
    }
}
