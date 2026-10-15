//! Guessing printer and material profiles from what a sliced file already
//! says about itself.
//!
//! Every supported format records which printer it was sliced for and
//! which filament sat in each slot — often down to the vendor's own
//! profile name and its price per kilo. This module turns that into
//! profiles so a job can be costed without the caller naming anything by
//! hand:
//!
//! 1. [`match_printer`] / [`match_material`] score the file's names
//!    against the [`crate::builtin`] presets (and any extra candidates
//!    the caller passes, e.g. the CLI's on-disk library).
//! 2. [`material_from_slot`] falls back to building a profile straight
//!    out of the slicer's own numbers when nothing matches but the file
//!    stated a price.
//!
//! Matching is deliberately conservative: it is a convenience, and a
//! wrong guess quietly mis-prices a job. Anything below
//! [`MIN_CONFIDENCE`] is reported as no match rather than a bad one, and
//! an explicit caller-supplied profile always wins.

use crate::formats::slice_summary::{SliceSummary, SlotUsage};
use crate::model::filament::{MaterialKind, MaterialProfile};
use crate::model::printer::{PrinterKind, PrinterProfile};

/// Score below which a candidate is discarded as "no match". Scores run
/// `0.0..=1.0`; this sits just above the level a shared word or two
/// reaches on its own.
pub const MIN_CONFIDENCE: f64 = 0.55;

/// A profile guessed from a sliced file, with how sure the guess is and
/// what it was guessed from.
///
/// `#[non_exhaustive]`: read-only output of the library.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Match<T> {
    /// The matched profile.
    pub profile: T,
    /// `0.0..=1.0`; `1.0` is an exact name match.
    pub confidence: f64,
    /// What in the sliced file produced the match, for showing the user
    /// — e.g. `"Bambu Lab P2S"` or `"Bambu PLA Basic @BBL P2S"`.
    pub matched_on: String,
}

/// The best-matching printer profile for a sliced file, or `None` if
/// nothing scored above [`MIN_CONFIDENCE`].
///
/// `candidates` is searched ahead of the built-in presets, so a caller
/// with its own library (the CLI) can have a saved profile win a tie.
/// Only profiles of the [`PrinterKind`] the file's format implies are
/// considered, so a resin slice never matches an FDM machine.
pub fn match_printer<'a>(
    slice: &SliceSummary,
    candidates: impl IntoIterator<Item = &'a PrinterProfile>,
    kind: PrinterKind,
) -> Option<Match<PrinterProfile>> {
    // Most specific first: the profile name pins the nozzle too, the bare
    // model doesn't.
    let hints: Vec<&str> = [slice.meta.printer_profile.as_deref(), slice.printer_model()]
        .into_iter()
        .flatten()
        .collect();
    if hints.is_empty() {
        return None;
    }

    let pool: Vec<&PrinterProfile> = candidates
        .into_iter()
        .chain(crate::builtin::printers())
        .filter(|p| p.kind == kind)
        .collect();

    best(&pool, &hints, |p| vec![p.name.as_str()]).map(|(profile, confidence, matched_on)| Match {
        profile: profile.clone(),
        confidence,
        matched_on,
    })
}

/// The best-matching material profile for one slot of a sliced file, or
/// `None` if nothing scored above [`MIN_CONFIDENCE`]. Same candidate and
/// kind rules as [`match_printer`].
pub fn match_material<'a>(
    slot: &SlotUsage,
    candidates: impl IntoIterator<Item = &'a MaterialProfile>,
    kind: MaterialKind,
) -> Option<Match<MaterialProfile>> {
    // The slicer's own profile name is by far the strongest signal;
    // vendor + family ("Bambu Lab" + "PLA") is a usable second.
    let vendor_and_type = match (slot.vendor.as_deref(), slot.material_type.as_deref()) {
        (Some(v), Some(t)) => Some(format!("{v} {t}")),
        _ => None,
    };
    let hints: Vec<&str> = [
        slot.profile_name.as_deref(),
        vendor_and_type.as_deref(),
        slot.material_type.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if hints.is_empty() {
        return None;
    }

    let pool: Vec<&MaterialProfile> = candidates
        .into_iter()
        .chain(crate::builtin::filaments())
        .filter(|m| {
            m.kind == kind && !vendors_conflict(slot.vendor.as_deref(), m.vendor.as_deref())
        })
        .collect();

    // A profile's vendor is part of its identity ("Bambu Lab" +
    // "bambu-pla-basic"), so match against both spellings.
    best(&pool, &hints, |m| match m.vendor.as_deref() {
        Some(vendor) => vec![m.name.as_str(), vendor],
        None => vec![m.name.as_str()],
    })
    .map(|(profile, confidence, matched_on)| Match {
        profile: profile.clone(),
        confidence,
        matched_on,
    })
}

/// Whether two vendor names are both stated and name different makers.
///
/// Without this, `"Generic PLA Silk"` matches `bambu-pla-silk` on two
/// shared words out of three — near enough to pass, and wrong. A stated
/// vendor that doesn't match is a hard veto; a missing one on either side
/// says nothing and vetoes nothing.
fn vendors_conflict(slot_vendor: Option<&str>, profile_vendor: Option<&str>) -> bool {
    match (slot_vendor, profile_vendor) {
        (Some(a), Some(b)) => similarity(a, b) == 0.0,
        _ => false,
    }
}

/// A material profile built straight from what the slicer said about a
/// slot — the last resort when no stored profile matches.
///
/// Needs a price: the file's `filament_cost` (per kg) or a resin bottle's
/// per-liter cost. Without one there's nothing to charge, so this returns
/// `None` rather than inventing a number. `efficiency_factor` is the
/// caller's default, since no slicer records a waste allowance.
pub fn material_from_slot(
    slot: &SlotUsage,
    kind: MaterialKind,
    efficiency_factor: f64,
) -> Option<MaterialProfile> {
    let price = slot.price.filter(|p| *p > 0.0)?;
    let name = slot
        .profile_name
        .clone()
        .or_else(|| slot.material_type.clone())
        .unwrap_or_else(|| format!("slot-{}", slot.slot_id));
    Some(MaterialProfile {
        name,
        kind,
        price,
        efficiency_factor,
        color: slot.color.clone(),
        vendor: slot.vendor.clone(),
    })
}

/// Scores every candidate against every hint and returns the best
/// (profile, score, hint) triple above [`MIN_CONFIDENCE`].
///
/// `names_of` yields the spellings a candidate answers to; a candidate
/// scores the best any of its spellings reaches against any hint. Earlier
/// hints are worth more than later ones (the list is most- to
/// least-specific), and ties go to the earliest candidate — which is why
/// callers put their own library ahead of the built-ins.
fn best<'a, T>(
    pool: &[&'a T],
    hints: &[&str],
    names_of: impl Fn(&'a T) -> Vec<&'a str>,
) -> Option<(&'a T, f64, String)> {
    let mut best: Option<(&'a T, f64, String)> = None;
    for (rank, hint) in hints.iter().enumerate() {
        // 1.0, 0.9, 0.81, ... — a match on a weaker hint is a weaker match.
        let hint_weight = 0.9_f64.powi(rank as i32);
        for candidate in pool {
            let score = names_of(candidate)
                .into_iter()
                .map(|name| similarity(name, hint))
                .fold(0.0_f64, f64::max)
                * hint_weight;
            if score >= MIN_CONFIDENCE && best.as_ref().is_none_or(|(_, b, _)| score > *b) {
                best = Some((*candidate, score, (*hint).to_string()));
            }
        }
    }
    best
}

/// Share of the score that comes from covering the shorter name; the
/// rest rewards covering the longer one too, so only identical token sets
/// reach `1.0`.
const SHORT_SIDE_WEIGHT: f64 = 0.85;

/// How alike two profile names are, `0.0..=1.0`.
///
/// Both sides are reduced to lowercase alphanumeric tokens first, so
/// `"Bambu PLA Basic @BBL P2S"`, `"bambu-pla-basic"` and
/// `"Bambu_PLA_Basic"` all compare as the same three words.
///
/// The score is mostly the share of the *shorter* side's tokens the other
/// side also has — asymmetric on purpose, since a stored profile name is
/// usually a shorter form of the slicer's (`"bambu-a1"` inside `"Bambu Lab
/// A1 0.4 nozzle"`). Coverage of the longer side contributes the
/// remaining [`SHORT_SIDE_WEIGHT`] fraction, so a name wholly contained in
/// a much longer one still scores well but stops short of `1.0` —
/// `"prusa-core-one"` against `"Prusa CORE One INDX 8T HF0.4 nozzle"` is a
/// good guess, not a certainty. Only identical token sets score `1.0`.
fn similarity(a: &str, b: &str) -> f64 {
    let a = tokens(a);
    let b = tokens(b);
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    // A one-token side is contained in anything sharing that token, which
    // identifies nothing — "PLA" would score full marks against every PLA
    // profile there is. Make it earn the match by being the whole of both
    // sides.
    if a.len().min(b.len()) == 1 && a.len() != b.len() {
        return 0.0;
    }
    let shared = a.iter().filter(|t| b.contains(t)).count() as f64;
    let short = shared / a.len().min(b.len()) as f64;
    let long = shared / a.len().max(b.len()) as f64;
    short * (SHORT_SIDE_WEIGHT + (1.0 - SHORT_SIDE_WEIGHT) * long)
}

/// Lowercase alphanumeric words, with a profile-name suffix (`"@BBL
/// P2S"`, `"@System"`, `"@0.05 SL1S"`) dropped — that part names the
/// printer the preset is tuned for, not the material, and keeping it
/// would let two unrelated filaments for the same machine look alike.
/// Pure-number tokens are kept: nozzle sizes and layer heights genuinely
/// distinguish printer profiles.
fn tokens(value: &str) -> Vec<String> {
    value
        .split('@')
        .next()
        .unwrap_or(value)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::slice_summary::{FilamentUsage, PlateSummary, SliceMeta, SourceFormat};
    use std::collections::HashMap;

    fn slice(printer_model: &str, printer_profile: Option<&str>) -> SliceSummary {
        SliceSummary {
            source_format: SourceFormat::Gcode,
            meta: SliceMeta {
                printer_profile: printer_profile.map(String::from),
                ..SliceMeta::default()
            },
            plates: vec![PlateSummary {
                plate_index: 1,
                print_time_seconds: 1000.0,
                filament_usage: vec![FilamentUsage {
                    slot_id: "1".into(),
                    grams: Some(10.0),
                    ..FilamentUsage::default()
                }],
                printer_model: Some(printer_model.to_string()),
                layer_height: None,
                extra: HashMap::new(),
            }],
        }
    }

    fn slot(profile_name: Option<&str>, vendor: Option<&str>, kind: Option<&str>) -> SlotUsage {
        SlotUsage {
            slot_id: "1".into(),
            material_type: kind.map(String::from),
            color: Some("#AE6342".into()),
            profile_name: profile_name.map(String::from),
            vendor: vendor.map(String::from),
            price: Some(24.99),
            grams: 10.0,
            ml: 0.0,
        }
    }

    #[test]
    fn tokens_drop_the_printer_suffix_and_punctuation() {
        assert_eq!(
            tokens("Bambu PLA Basic @BBL P2S"),
            ["bambu", "pla", "basic"]
        );
        assert_eq!(tokens("bambu-pla-basic"), ["bambu", "pla", "basic"]);
        assert_eq!(
            tokens("Prusa MINI 0.4 nozzle"),
            ["prusa", "mini", "0", "4", "nozzle"]
        );
    }

    /// Only identical token sets score 1.0; containment scores high but
    /// short of it, and nothing in common scores 0.
    #[test]
    fn similarity_is_one_only_for_the_same_tokens() {
        assert_eq!(
            similarity("bambu-pla-basic", "Bambu PLA Basic @BBL P2S"),
            1.0
        );
        assert_eq!(similarity("PLA", "pla"), 1.0);

        let contained = similarity("prusa-core-one", "Prusa CORE One INDX 8T HF0.4 nozzle");
        assert!(
            (MIN_CONFIDENCE..1.0).contains(&contained),
            "containment is a strong guess, not a certainty: {contained}"
        );

        assert_eq!(similarity("prusa-mk4s", "Bambu Lab P2S"), 0.0);
        // A lone family name identifies nothing on its own.
        assert_eq!(similarity("PLA", "pla-black"), 0.0);
    }

    #[cfg(feature = "bambu")]
    #[test]
    fn printer_matches_the_model_the_file_names() {
        let matched = match_printer(&slice("Bambu Lab P2S", None), [], PrinterKind::Fdm);
        let matched = matched.expect("P2S is a built-in preset");
        assert_eq!(matched.profile.name, "bambu-p2s");
        assert_eq!(matched.matched_on, "Bambu Lab P2S");
    }

    #[test]
    fn printer_with_nothing_recognizable_is_no_match() {
        assert!(
            match_printer(&slice("Some Unknown Machine", None), [], PrinterKind::Fdm).is_none()
        );
    }

    /// A resin slice can never match an FDM printer, even by name.
    #[test]
    fn printer_match_respects_kind() {
        assert!(
            match_printer(&slice("generic-resin", None), [], PrinterKind::Fdm).is_none(),
            "an FDM search must not return the resin preset"
        );
        assert!(match_printer(&slice("generic-resin", None), [], PrinterKind::Resin).is_some());
    }

    #[cfg(feature = "bambu")]
    #[test]
    fn material_matches_the_slicer_profile_name() {
        let matched = match_material(
            &slot(
                Some("Bambu PLA Basic @BBL P2S"),
                Some("Bambu Lab"),
                Some("PLA"),
            ),
            [],
            MaterialKind::Filament,
        )
        .expect("Bambu PLA Basic is a built-in preset");
        assert_eq!(matched.profile.name, "bambu-pla-basic");
        assert_eq!(matched.confidence, 1.0);
    }

    /// A caller's own profile is searched first, so it wins an exact tie
    /// against a built-in preset of the same name.
    #[test]
    fn caller_candidates_win_ties() {
        let mine = MaterialProfile {
            name: "pla-black".into(),
            kind: MaterialKind::Filament,
            price: 9.99,
            efficiency_factor: 1.0,
            color: None,
            vendor: None,
        };
        let matched = match_material(
            &slot(Some("pla black"), None, Some("PLA")),
            [&mine],
            MaterialKind::Filament,
        )
        .expect("exact name match");
        assert_eq!(matched.profile.price, 9.99);
    }

    /// "Generic PLA Silk" shares two words out of three with
    /// `bambu-pla-silk` — close enough to pass the score threshold, and
    /// wrong. The stated vendor is what rules it out.
    #[cfg(feature = "bambu")]
    #[test]
    fn a_different_stated_vendor_vetoes_a_close_name() {
        assert!(
            match_material(
                &slot(
                    Some("Generic PLA Silk @COREONEINDX HF0.4"),
                    Some("Generic"),
                    Some("PLA")
                ),
                [],
                MaterialKind::Filament,
            )
            .is_none()
        );
        // Same name, no vendor stated: nothing to veto with, so the close
        // name is allowed to match.
        assert!(
            match_material(
                &slot(Some("PLA Silk"), None, Some("PLA")),
                [],
                MaterialKind::Filament,
            )
            .is_some()
        );
    }

    #[test]
    fn material_from_slot_uses_the_files_own_price() {
        let built = material_from_slot(
            &slot(Some("Generic PLA @System"), Some("Generic"), Some("PLA")),
            MaterialKind::Filament,
            1.1,
        )
        .expect("the slot carries a price");
        assert_eq!(built.name, "Generic PLA @System");
        assert_eq!(built.price, 24.99);
        assert_eq!(built.efficiency_factor, 1.1);
        assert_eq!(built.vendor.as_deref(), Some("Generic"));
    }

    /// No price in the file means no profile — never a silently free one.
    #[test]
    fn material_from_slot_needs_a_price() {
        let unpriced = SlotUsage {
            price: None,
            ..slot(Some("Generic PLA"), None, Some("PLA"))
        };
        assert!(material_from_slot(&unpriced, MaterialKind::Filament, 1.1).is_none());
    }
}
