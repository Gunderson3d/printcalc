//! Built-in printer/filament presets — plain in-memory data, no
//! filesystem involved, so (unlike [`crate::disk`]) these are available
//! to `printcalc-wasm` too. [`crate::ops::show_printer`]/[`crate::ops::
//! show_filament`] (and anything that resolves a `Linked` ref through
//! them) fall back to these when a name isn't found in the local
//! library.
//!
//! Starter data — swap in a real catalog.

use crate::model::filament::{FilamentProfile, MaterialKind};
use crate::model::printer::{PrinterKind, PrinterProfile, WearProfile};

/// Every built-in printer preset.
pub fn printers() -> Vec<PrinterProfile> {
    vec![
        PrinterProfile {
            name: "bambu-a1".to_string(),
            kind: PrinterKind::Fdm,
            wear: WearProfile {
                purchase_price: 800.0,
                additional_upfront_cost: 0.0,
                estimated_life_years: 5.0,
                estimated_uptime_percent: 0.5,
                annual_maintenance_cost: 80.0,
                power_consumption_watts: 160.0,
                cost_buffer_factor: 1.3,
            },
        },
        PrinterProfile {
            name: "generic-resin".to_string(),
            kind: PrinterKind::Resin,
            wear: WearProfile {
                purchase_price: 400.0,
                additional_upfront_cost: 0.0,
                estimated_life_years: 5.0,
                estimated_uptime_percent: 0.3,
                annual_maintenance_cost: 20.0,
                power_consumption_watts: 60.0,
                cost_buffer_factor: 1.2,
            },
        },
    ]
}

/// A built-in printer preset by name.
pub fn printer(name: &str) -> Option<PrinterProfile> {
    printers().into_iter().find(|p| p.name == name)
}

/// Every built-in filament preset.
pub fn filaments() -> Vec<FilamentProfile> {
    vec![
        FilamentProfile {
            name: "pla-black".to_string(),
            kind: MaterialKind::Filament,
            price_per_kg: Some(18.99),
            price_per_liter: None,
            density_g_per_cm3: None,
            efficiency_factor: 1.1,
            color: Some("black".to_string()),
            vendor: None,
        },
        FilamentProfile {
            name: "generic-resin-grey".to_string(),
            kind: MaterialKind::Resin,
            price_per_kg: None,
            price_per_liter: Some(40.0),
            density_g_per_cm3: None,
            efficiency_factor: 1.0,
            color: Some("grey".to_string()),
            vendor: None,
        },
    ]
}

/// A built-in filament preset by name.
pub fn filament(name: &str) -> Option<FilamentProfile> {
    filaments().into_iter().find(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names_resolve_and_unknown_names_dont() {
        assert_eq!(printer("bambu-a1").unwrap().kind, PrinterKind::Fdm);
        assert!(printer("nonexistent").is_none());
        assert_eq!(filament("pla-black").unwrap().kind, MaterialKind::Filament);
        assert!(filament("nonexistent").is_none());
    }
}
