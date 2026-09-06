//! `printcalc filament` — add, list and inspect saved filament and resin
//! profiles.
//!
//! One command covers both: `--kind` decides whether `--price` is read
//! per kilogram (filament, measured in grams) or per litre (resin,
//! measured in millilitres). As with printers, only a name is required.
use std::error::Error;

use clap::{Args, Subcommand};

use libprintcalc::model::filament::{
    DEFAULT_EFFICIENCY_FACTOR, DEFAULT_PRICE, MaterialKind, MaterialProfile,
};

use crate::interactive;
use crate::ops;

#[derive(Args)]
pub struct FilamentArgs {
    #[command(subcommand)]
    pub command: FilamentCommand,
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
        /// "filament" or "resin".
        #[arg(short, long, default_value = "filament")]
        kind: String,
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
        FilamentCommand::Add {
            name,
            kind,
            price,
            efficiency_factor,
            color,
            vendor,
        } => {
            let name =
                interactive::require_string(name, "name", "Filament name", interactive_mode)?;
            let kind = match kind.as_str() {
                "filament" => MaterialKind::Filament,
                "resin" => MaterialKind::Resin,
                other => {
                    return Err(format!(
                        "invalid --kind {other:?} (expected \"filament\" or \"resin\")"
                    )
                    .into());
                }
            };
            let price_unit = match kind {
                MaterialKind::Filament => "kg",
                MaterialKind::Resin => "liter",
            };
            let price = interactive::defaulted_f64(
                price,
                &format!("Price per {price_unit}"),
                DEFAULT_PRICE,
                interactive_mode,
            )?;
            let efficiency_factor = interactive::defaulted_f64(
                efficiency_factor,
                "Efficiency factor",
                DEFAULT_EFFICIENCY_FACTOR,
                interactive_mode,
            )?;
            let profile = MaterialProfile {
                name: name.clone(),
                kind,
                price,
                efficiency_factor,
                color,
                vendor,
            };
            ops::add_filament(&profile)?;
            println!("saved filament {name:?}");
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

fn print_filament(f: &MaterialProfile) {
    println!("name: {}", f.name);
    println!("kind: {:?}", f.kind);
    let price_unit = match f.kind {
        MaterialKind::Filament => "kg",
        MaterialKind::Resin => "liter",
    };
    println!("price: {} per {price_unit}", f.price);
    println!("efficiency_factor: {}", f.efficiency_factor);
    if let Some(c) = &f.color {
        println!("color: {c}");
    }
    if let Some(v) = &f.vendor {
        println!("vendor: {v}");
    }
}
