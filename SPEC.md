# PrintCalc — Spec / Plan

CLI tool (+ WASM) to calculate 3D print job costs: material, print time, printer
wear, electricity, labor, and extra hardware. Reads sliced output files
directly to pull real numbers instead of requiring manual entry. The core
calculation/parsing logic is a standalone library crate (`printcalc-lib`),
usable on its own by other Rust projects — the CLI and WASM build are just
two frontends over it.

## Goals

- Support FDM and resin slicers. v1 targets: Bambu Studio (`.gcode.3mf`),
  PrusaSlicer FDM (`.gcode` and `.bgcode`), PrusaSlicer resin (`.sl1` /
  `.sl1s`).
- Modular compilation — a user who only prints on a Bambu printer shouldn't
  need to compile Prusa parsing code, and vice versa. Features are organized
  per-vendor as well as per-format: enabling the `prusa` feature pulls in
  every Prusa format (gcode, bgcode, sl1/sl1s) without having to name each
  one, while each format can also be selected individually for finer-grained
  builds.
- Also compiles to WebAssembly for in-browser use.
- Job records save as TOML, in three tiers of portability (see below).
- Users define their own printer and filament/resin profiles, saved as TOML,
  reused across jobs.
- Accounts for material cost, printer depreciation, electricity, labor, and
  arbitrary extra hardware costs (inserts, magnets, etc).
- Suggests sale prices at configurable markup percentages (defaults: 50%,
  60%, 70%), shown alongside the cost breakdown in the CLI summary.
- Bambu `.gcode.3mf` files can contain multiple plates (one build-plate
  arrangement per `<plate>` block); supports costing a single chosen plate
  or all plates combined (see §2, §3).
- Multi-filament costing for printers that can lay down more than one
  material per plate — Bambu AMS (multiple `<filament>` slots per plate)
  and multi-extruder Prusa printers (XL, Core One IDEX) — every filament
  slot is priced against its own filament profile via a `slot_id -> profile`
  map. The common single-material case is just a one-entry map under a
  fixed slot id, with `--filament` / `[filament]` kept as sugar so the
  common case doesn't need a slot id spelled out by hand (see §2, §3, §5).
- `printcalc-lib` is designed to be published and used as a general-purpose
  library (e.g. via crates.io) by other people's Rust projects, not just
  consumed internally by this workspace's CLI/WASM crates — stable public
  API and documentation are first-class concerns, not an afterthought.

Out of scope for v1 (revisit later if wanted): currency conversion.

## 1. Workspace layout

Cargo workspace of three crates. Format parsers live inside `printcalc-lib`
behind feature flags, so `printcalc-cli` and `printcalc-wasm` each pull in only
the parsers (and only the target) they need at build time.

```
printcalc/                        (workspace root)
  Cargo.toml
  printcalc-lib/
    Cargo.toml                  # [features] vendor umbrellas: bambu, prusa
                                 #            formats: bambu-3mf, prusa-gcode,
                                 #                      prusa-sl1, prusa-bgcode
                                 # default = ["bambu", "prusa"] (= all formats)
    src/
      lib.rs
      model/
        mod.rs
        printer.rs              # PrinterProfile
        filament.rs             # FilamentProfile
        job.rs                  # Job; Default/Full/Bundled tiers fall out of
                                 # ProfileRef (Linked/Embedded) & JobSource
                                 # (Path/Embedded) — not a separate tier enum
        cost.rs                 # CostBreakdown + calculation
      formats/
        mod.rs                  # SliceParser trait, detect_format()
        slice_summary.rs        # SliceSummary (the common extracted-data shape)
        bambu_3mf.rs            #[cfg(feature = "bambu-3mf")]
        prusa_gcode.rs          #[cfg(feature = "prusa-gcode")]
        prusa_sl1.rs            #[cfg(feature = "prusa-sl1")]
        prusa_bgcode.rs         #[cfg(feature = "prusa-bgcode")]
        prusa_metadata.rs       # shared key=value parsing used by both
                                 # prusa_gcode and prusa_bgcode
      library.rs                # profile library load/save, global config
      ops.rs                    # orchestration fns: new_job, calc_job, bundle_job,
                                 # add/list/show printer & filament profiles
  printcalc-cli/
    Cargo.toml                  # bin; depends on printcalc-lib, feature-passthrough
    src/
      main.rs
      cli/
        job.rs                  # arg parsing + printing only; calls printcalc_lib::ops::*
        printer.rs               # arg parsing + printing only; calls printcalc_lib::ops::*
        filament.rs              # arg parsing + printing only; calls printcalc_lib::ops::*
      interactive.rs            # shared prompt helpers for --interactive
  printcalc-wasm/
    Cargo.toml                  # cdylib, wasm-bindgen; depends on printcalc-lib
    src/lib.rs                  # thin bindgen wrapper around the same printcalc_lib::ops::* calls
```

**Principle:** all real logic — parsing, cost calc, profile CRUD, job-tier
conversion — lives in `printcalc_lib::ops`. Neither `printcalc-cli` nor
`printcalc-wasm` should contain business logic of their own; they're both thin
frontends (arg parsing + interactive prompts + terminal output vs. JS
bindings) over the same `ops` functions. This keeps the two frontends from
drifting apart and means anything the CLI can do, the WASM build can do too
(filesystem-dependent parts of `library.rs`, like resolving `ref =` from
disk, aside — WASM callers pass profiles in directly since there's no
filesystem).

Default build (no flags needed) includes all formats:
`cargo build -p printcalc-cli`

Slimming down by vendor, e.g. a Prusa-only CLI (gcode + bgcode + sl1/sl1s):
`cargo build -p printcalc-cli --no-default-features --features prusa`

Slimming down to one specific format, e.g. resin only:
`cargo build -p printcalc-cli --no-default-features --features prusa-sl1`

Vendor features are just a convenience union of their formats (`bambu =
["bambu-3mf"]`, `prusa = ["prusa-gcode", "prusa-sl1", "prusa-bgcode"]`) —
picking the vendor feature or naming the format features individually
produces the same build.

The wasm crate ships with all formats enabled by default too, same rationale
— nothing stops publishing a slimmer `fdm-only` wasm bundle the same way if
that's ever wanted.

## 2. Core data model (sketch)

```rust
// formats/slice_summary.rs
pub struct SliceSummary {
    pub source_format: SourceFormat,       // Bambu3mf, PrusaGcode, PrusaBgcode, PrusaSl1
    pub plates: Vec<PlateSummary>,         // always >= 1; single-plate formats
                                            // (Prusa gcode, sl1/sl1s) produce
                                            // exactly one entry
}

pub struct PlateSummary {
    pub plate_index: u32,                  // 1-based, matches the slicer's own numbering
    pub print_time_seconds: f64,
    pub filament_usage: Vec<FilamentUsage>, // per-slot/per-vat material usage — the only
                                            // source of truth for what was used on this
                                            // plate. Single-material prints (including
                                            // every resin format) still produce exactly
                                            // one entry. There's no separate summed-total
                                            // or material-name field to keep in sync —
                                            // total grams/ml and a display name (when
                                            // there's just one distinct filament) are
                                            // derived from this list wherever needed.
    pub printer_model: Option<String>,
    pub layer_height: Option<f64>,
    pub extra: HashMap<String, String>,    // anything format-specific worth keeping
}

// One entry per filament/resin slot that actually deposited material on
// this plate — a plain single-material print has exactly one entry; Bambu
// AMS or multi-extruder Prusa (XL, Core One IDEX) prints have one entry
// per tray/extruder used.
pub struct FilamentUsage {
    pub slot_id: String,          // opaque, format-native id: Bambu AMS tray/`id`
                                   // attribute, Prusa extruder index — whatever the
                                   // slicer itself uses to distinguish slots
    pub material_type: Option<String>,
    pub color: Option<String>,
    pub grams: Option<f64>,
    pub ml: Option<f64>,
}

// formats/mod.rs
pub trait SliceParser {
    fn detect(bytes: &[u8], filename: &str) -> bool;
    fn parse(bytes: &[u8]) -> Result<SliceSummary, ParseError>;
}
```

```rust
// model/printer.rs
pub struct PrinterProfile {
    pub name: String,
    pub kind: PrinterKind,          // Fdm, Resin
    pub wear: WearProfile,
}

pub struct WearProfile {
    pub purchase_price: f64,
    pub additional_upfront_cost: f64,
    pub estimated_life_years: f64,
    pub estimated_uptime_percent: f64,   // 0.0-1.0
    pub annual_maintenance_cost: f64,
    pub power_consumption_watts: f64,    // the printer's own physical power draw
    pub cost_buffer_factor: f64,
}
// Electricity price isn't a printer property — it's wherever you pay your
// bill from, which can vary job to job (time-of-day rate, a different
// circuit/outlet) independent of which printer ran it. It lives on Job
// instead (electricity_cost_per_kwh_override, below), falling back to a
// global config default.

// model/filament.rs
pub struct FilamentProfile {
    pub name: String,
    pub kind: MaterialKind,          // Filament, Resin
    pub price_per_kg: Option<f64>,   // filament
    pub price_per_liter: Option<f64>,// resin
    pub density_g_per_cm3: Option<f64>,
    pub efficiency_factor: f64,      // default waste multiplier, e.g. 1.1
    pub color: Option<String>,
    pub vendor: Option<String>,
}

// model/job.rs
pub enum ProfileRef<T> {
    Linked { name: String },   // resolved against local library at calc time
    Embedded(Box<T>),
}

pub struct Job {
    pub name: String,
    pub source: JobSource,                       // path, or embedded SliceSummary
    // None => every plate; Some(i) => just plate i. A sibling of `source`
    // here (not nested inside the JobSource enum, which would duplicate it
    // across the Path/Embedded variants) — but it still serializes under
    // the `[source]` TOML table (`[source] plate = ...`, §5) since that's
    // the natural place for "which part of the source to cost" to read.
    pub plate_selection: Option<u32>,
    pub printer: ProfileRef<PrinterProfile>,
    // Keyed by FilamentUsage::slot_id. Every slot with nonzero usage on
    // the selected plate(s) must have an entry, or calc fails naming the
    // missing slot. Single-material jobs are just a one-entry map under
    // a fixed slot id ("1") — `--filament` / `[filament]` (§5, §7) are
    // sugar over this one map, so the common case never has to name a
    // slot id explicitly.
    pub filament: HashMap<String, ProfileRef<FilamentProfile>>,
    pub material_efficiency_factor_override: Option<f64>,
    pub electricity_cost_per_kwh_override: Option<f64>, // None => use global config default
    pub labor: LaborCost,
    pub hardware: Vec<HardwareItem>,
    pub markup_percentages_override: Option<Vec<f64>>, // None => use global config default
}

pub enum JobSource {
    Path(PathBuf),           // Default & Full: re-parse the sliced file at calc time
    Embedded(SliceSummary),  // Bundled: no external file needed
}

pub struct LaborCost {
    pub rate_per_hour: Option<f64>, // None => use global config default
    pub minutes: f64,               // no sensible global default; always job-specific
}

pub struct HardwareItem {
    pub name: String,
    pub cost: f64,
    pub quantity: u32,
}

// model/cost.rs
pub struct CostBreakdown {
    pub material_cost: f64,
    pub wear_cost: f64,
    pub electricity_cost: f64,
    pub labor_cost: f64,
    pub hardware_cost: f64,
    pub total_cost: f64,
    pub suggested_prices: Vec<SuggestedPrice>,  // one per markup percentage
}

pub struct SuggestedPrice {
    pub markup_percent: f64,
    pub price: f64,
}
```

## 3. Cost formulas

Derived from an existing pricing worksheet used as a reference model,
validated against its example numbers (an 800 dollar
printer, 5yr life, 50% uptime, 160W, 0.14/kWh, 1.3 buffer → 0.05 wear/hr +
0.02 electric/hr → 0.10/hr total).

**Plate selection (applied first):**
```
selected_plates = match job.plate_selection {
    None     => slice_summary.plates.iter().collect(),                 // every plate
    Some(i)  => vec![slice_summary.plates.iter().find(|p| p.plate_index == i)
                      .expect("plate index not found in sliced file")],
}

total_print_time_seconds = sum(p.print_time_seconds for p in selected_plates)
```
For single-plate formats (Prusa gcode, `.sl1`/`.sl1s`) there's exactly one
plate either way, so `None` and `Some(1)` are equivalent. `job calc` shows a
per-plate line-item breakdown when `None` resolves to more than one plate,
plus the combined total (see §7 for an example). There's no separate
material-total step here — the material cost formula below iterates
`filament_usage` across `selected_plates` directly.

**Material cost:** one path, always — costed per `FilamentUsage` entry
across the selected plate(s) against the map in `job.filament`, then
summed. A single-material job is just this loop running over a
one-element `filament_usage` and a one-entry `job.filament` map (slot
`"1"`), so there's no separate single-filament formula to keep in sync:
```
material_cost = 0
for plate in selected_plates:
    for usage in plate.filament_usage:
        if (usage.grams or usage.ml) is 0/None: continue   // slot present but unused, skip
        profile = job.filament.get(usage.slot_id)
                  .expect("no filament profile mapped for slot {usage.slot_id}")
        effective_factor = job.material_efficiency_factor_override.unwrap_or(profile.efficiency_factor)
        used_amount = (usage.grams or usage.ml) * effective_factor
        material_cost += used_amount * (profile.price_per_kg/1000 or profile.price_per_liter/1000)
```
`job calc` shows a single `Material` line when only one profile
contributed to the total (true for every single-material job, and for
multi-slot jobs where every slot happens to share one profile), and one
`Material (<name>)` line per distinct profile otherwise (see §7).

**Printer wear + electricity:**
```
total_investment      = purchase_price + additional_upfront_cost
lifetime_cost         = total_investment + (annual_maintenance_cost * estimated_life_years)
uptime_hours_per_year = 8760 * estimated_uptime_percent
capital_cost_per_hour = lifetime_cost / (estimated_life_years * uptime_hours_per_year)

electricity_rate       = job.electricity_cost_per_kwh_override.unwrap_or(global_config.electricity_cost_per_kwh)
electrical_cost_per_hour = (power_consumption_watts / 1000) * electricity_rate

print_hours = total_print_time_seconds / 3600

wear_cost        = capital_cost_per_hour   * cost_buffer_factor * print_hours
electricity_cost = electrical_cost_per_hour * cost_buffer_factor * print_hours
```
(Shown as two line items that sum to `printer_rate_per_hour * print_hours`,
matching the worksheet's combined "Total Printer Cost per hour".)

**Labor cost:**
```
effective_rate_per_hour = job.labor.rate_per_hour.unwrap_or(global_config.labor_rate_per_hour)
labor_cost = effective_rate_per_hour * (job.labor.minutes / 60)
```

**Hardware cost:**
```
hardware_cost = sum(item.cost * item.quantity for item in hardware)
```

**Total:**
```
total_cost = material_cost + wear_cost + electricity_cost + labor_cost + hardware_cost
```

**Suggested sale prices (markup):**
```
markup_percentages = job.markup_percentages_override.unwrap_or(global_config.markup_percentages)
// default: [50.0, 60.0, 70.0]

for pct in markup_percentages:
    suggested_price = total_cost * (1 + pct / 100)
```

## 4. File format parsers

| Format | Feature | Container | Key fields extracted |
|---|---|---|---|
| Bambu `.gcode.3mf` | `bambu-3mf` | Zip (3MF), embedded per-plate gcode + `Metadata/slice_info.config` (XML), `Metadata/project_settings.config` (JSON) | one `<plate>` block per plate, each with its own `prediction` (print time, seconds), `weight` (g), one or more `<filament>` elements (`id`/type/color/`used_g`), and `printer_model_id`; each `<filament>`'s `id` attribute is the AMS tray index and becomes that plate's `FilamentUsage::slot_id` |
| Prusa `.gcode` | `prusa-gcode` | Plain text, slicer-generated comment footer (`; estimated printing time... = `, `; filament used [g] = `, `; filament_type = `) | print time; filament grams/type (single value for single-extruder prints, comma-separated per-extruder on multi-extruder Prusa printers — XL, Core One IDEX — split into one `FilamentUsage` per nonzero extruder, slot_id = position in the list) |
| Prusa `.bgcode` | `prusa-bgcode` | Binary block container (magic `GCDE`); a metadata block holds the same `key=value` text as the `.gcode` footer, ahead of compressed gcode blocks | same fields and same comma-separated multi-extruder handling as `.gcode`, different container |
| Prusa `.sl1` / `.sl1s` | `prusa-sl1` | Zip, `config.ini` / `config.json` | `printTime` (seconds), `usedMaterial` (ml), `materialName`, `printerModel`, `layerHeight` (single plate, single vat — no multi-material concept for resin) |

All four rows are now confirmed against real sample files in `Testfiles/`:
`Shape-Box.sl1s` / `Shape-Box.sl1` (resin), `Cube_0.2mm_PLA_Prusa MINI_20m7s.gcode`
(Prusa FDM plain gcode), `Shape-Box_0.4n_0.2mm_PLA_MINIIS_24m.bgcode` (Prusa
binary gcode), and `single_plate.gcode.3mf` / `multi_plate.gcode.3mf` (Bambu,
one and two plates respectively). No more unverified assumptions here. The
multi-filament path is confirmed too: `single_plate_multicolor.gcode.3mf` /
`multi_plate_multicolor.gcode.3mf` (Bambu, two `<filament id=...>` entries
per plate) and `Shape-Box_0.4n_0.2mm_PLA_COREONEINDX_19m.gcode` /
`"Shape-Box_0.4n_0.2mm_PLA,PLA_COREONEINDX_19m.gcode"` (Prusa Core One IDEX,
single- vs. dual-extruder slicing of the same model — comma-separated
`filament used [g]` list, one value per extruder slot, unused slots at
`0.00`).

**`.gcode` / `.bgcode` share a metadata schema** — both formats' key=value
metadata (`filament used [g]=`, `estimated printing time (normal mode)=`,
`printer_model=`, `filament_type=`, ...) is textually identical, just one is
gcode comments and the other is a binary metadata block. `prusa_metadata.rs`
holds the one key=value parser both `prusa_gcode.rs` and `prusa_bgcode.rs`
call — `prusa_bgcode.rs` only adds the binary block framing (header,
per-block compression/checksum) on top before handing the metadata block's
text to the shared parser. On multi-extruder printers (XL, Core One IDEX)
several of these keys become comma-separated lists, one value per extruder
slot; `prusa_metadata.rs` detects the comma-separated shape and emits one
`FilamentUsage` per nonzero entry, using the position in the list (0-based)
as `slot_id`. A plain scalar (non-list) value — the single-extruder case —
still produces exactly one `FilamentUsage`, normalized to `slot_id = "1"` so
it lines up with Bambu's single-tray convention and the `--filament` /
`[filament]` sugar (§5, §7), which always targets slot `"1"`.

**Bambu multi-plate / multi-filament structure** — `Metadata/slice_info.config`
contains one `<plate>` element per plate, each with one `<filament>` element
per AMS tray actually used, confirmed against `single_plate_multicolor.gcode.3mf`:
```xml
<plate>
  <metadata key="index" value="1"/>
  <metadata key="prediction" value="1251"/>   <!-- seconds -->
  <metadata key="weight" value="8.70"/>       <!-- grams, sum of every <filament> below -->
  <filament id="1" type="PLA" color="#AE6342" used_g="4.83" .../>
  <filament id="2" type="PLA" color="#F8F4EF" used_g="3.87" .../>
</plate>
<plate>
  <metadata key="index" value="2"/>
  ...
</plate>
```
Each `<filament>` element becomes its own `FilamentUsage` (slot_id = its
`id` attribute, grams = its `used_g`) — `<metadata key="weight">` is the
slicer's own total and isn't separately stored, since it's just the sum of
the `<filament>` elements already captured.

**Prusa multi-extruder example** — same keys as single-extruder Prusa gcode,
just comma-separated, one value per extruder slot in tool-head order.
Confirmed against `"Shape-Box_0.4n_0.2mm_PLA,PLA_COREONEINDX_19m.gcode"`
(Core One IDEX, dual-extruder slice of the same model as the single-extruder
`Shape-Box_0.4n_0.2mm_PLA_COREONEINDX_19m.gcode`):
```
; filament used [g] = 1.38, 1.47, 0.00, 0.00, 0.00, 0.00, 0.00, 0.00
; filament_type = PLA;PLA;PLA;PLA;PLA;PLA;PLA;PLA
; filament_colour = #FF8000;#FF8000;#FF8000;#FF8000;#FF8000;#FF8000;#FF8000;#FF8000
```
Entries at `0.00` (extruder not used on this print) are skipped —
`FilamentUsage` is only produced for extruders that actually deposited
material, the same rule as Bambu AMS trays with zero usage. Note
`filament_colour`/`filament_type` are per-slot-position lists too but don't
by themselves distinguish *which* physical filament spool was loaded in
each slot beyond color/material — same limitation as Bambu's `color`/`type`
attributes, fine for this spec's purposes since the user names the profile
mapped to each slot anyway.

Format detection: by extension first, falling back to content sniffing —
zip magic bytes `PK\x03\x04` (3MF or sl1/sl1s, disambiguated by whether
`config.ini` or `3D/3dmodel.model` is present inside), the `GCDE` magic for
`.bgcode`, or plain-text gcode comments otherwise — so renamed files still
work.

## 5. TOML schemas

**Printer profile** (`~/.config/printcalc/printers/bambu-a1.toml`):
```toml
name = "bambu-a1"
kind = "fdm"

[wear]
purchase_price = 800.0
additional_upfront_cost = 0.0
estimated_life_years = 5.0
estimated_uptime_percent = 0.5
annual_maintenance_cost = 80.0
power_consumption_watts = 160.0
cost_buffer_factor = 1.3
```

**Filament profile** (`~/.config/printcalc/filaments/pla-black.toml`):
```toml
name = "pla-black"
kind = "filament"
price_per_kg = 18.99
efficiency_factor = 1.1
color = "black"
vendor = "Prusament"
```

**Global config** (`~/.config/printcalc/config.toml`):
```toml
electricity_cost_per_kwh = 0.14
labor_rate_per_hour = 15.0
markup_percentages = [50.0, 60.0, 70.0]
```

**Job — Default/minimal** (printer & filament linked by name; requires the
library and the original sliced file to be present at calc time):
```toml
name = "widget-v2"

[source]
path = "prints/widget.gcode.3mf"
# plate = "all"   # optional; "all" (default) sums every plate, or a number (e.g. 2) costs just that plate

[printer]
ref = "bambu-a1"

[filament]
ref = "pla-black"

# electricity_cost_per_kwh = 0.22   # optional; overrides global config for just this job
                                    # (e.g. this print ran on a different tariff/circuit)

[labor]
minutes = 10.0
# rate_per_hour = 15.0   # optional; omitted => use global config default

[[hardware]]
name = "brass insert M3x5"
cost = 0.12
quantity = 4

# markup_percentages = [50.0, 60.0, 70.0]  # optional; omitted => use global config default
```

`[filament]` above is sugar over the underlying `filament` map (§2) — it
deserializes to a one-entry map under slot `"1"`, exactly matching the
`slot_id` a single-material sliced file's lone `FilamentUsage` normalizes
to (§4). Nothing reads or writes a real multi-slot map for this common
case.

**Job — Default/minimal, multi-filament (Bambu AMS / Prusa multi-extruder)**
— `[filaments]` (plural, a table keyed by slot_id) is used directly instead
of the `[filament]` sugar when different slots should be priced against
different profiles:
```toml
name = "multicolor-vase"

[source]
path = "prints/vase_ams.gcode.3mf"

[printer]
ref = "bambu-a1"

[filaments]
"1" = { ref = "pla-black" }
"2" = { ref = "pla-red" }
"4" = { ref = "petg-clear" }
# slot "3" omitted here because it has 0g usage on every selected plate —
# only slots with nonzero usage require a mapping; calc fails naming the
# slot if one that's actually used is left unmapped

[labor]
rate_per_hour = 15.0
minutes = 10.0
```
`[filament]` (singular, sugar for a one-entry map) and `[filaments]`
(plural, the map spelled out directly) are just two ways to write the same
underlying `HashMap<String, ProfileRef<FilamentProfile>>` — there's no
separate variant to keep in sync, so a job file can only ever have one or
the other. The same relationship carries through Full/Bundled tiers below,
where `[filaments."1".profile]` etc. embed each slot's profile instead of
linking it by name.

**Job — Full** (printer & filament embedded; sliced file still linked by path):
```toml
name = "widget-v2"

[source]
path = "prints/widget.gcode.3mf"

[printer.profile]
name = "bambu-a1"
kind = "fdm"
[printer.profile.wear]
purchase_price = 800.0
# ...full wear block...

[filament.profile]
name = "pla-black"
kind = "filament"
price_per_kg = 18.99
efficiency_factor = 1.1

[labor]
rate_per_hour = 15.0
minutes = 10.0
```
`[filament.profile]` is the same sugar as `[filament] ref = ...` in
Default, just embedding rather than linking — it deserializes to a
one-entry map under slot `"1"`. A multi-filament Full job spells the map
out directly instead, one embedded profile per slot:
```toml
[filaments."1".profile]
name = "pla-black"
kind = "filament"
price_per_kg = 18.99
efficiency_factor = 1.1

[filaments."2".profile]
name = "pla-red"
kind = "filament"
price_per_kg = 18.99
efficiency_factor = 1.1
```

**Job — Bundled** (fully self-contained; the extracted slice summary is
embedded so the original sliced file is no longer needed at all):
```toml
name = "widget-v2"

[source]
# plate = "all"   # same meaning as in Default/Full

[source.summary]
source_format = "bambu-3mf"

[[source.summary.plates]]
plate_index = 1
print_time_seconds = 1037.0
[[source.summary.plates.filament_usage]]
slot_id = "1"
material_type = "PLA"
grams = 6.66

[[source.summary.plates]]
plate_index = 2
print_time_seconds = 988.0
[[source.summary.plates.filament_usage]]
slot_id = "1"
material_type = "PLA"
grams = 5.15

[printer.profile]
# ...embedded, same as Full...

[filament.profile]
# ...embedded, same as Full...

[labor]
rate_per_hour = 15.0
minutes = 10.0
```
`filament_usage` is the only place material data lives on `PlateSummary` —
a Bundled job's `filament_usage` array must have one entry per slot
(matching whatever slot ids `[filaments]` maps against) or calc has
nothing to loop over. Total grams/ml and a display name, if `job calc`
wants them, are computed by summing/reading this list at display time —
there's no separate stored total to fall out of sync with it.

`ref` (linked) vs `profile` (embedded) map naturally to a serde untagged enum
on `ProfileRef<T>`; `path` vs `summary` under `[source]` similarly distinguish
Default/Full from Bundled. `filament` vs `filaments` aren't a data-model
distinction at all, just two TOML shapes for the same `filament` map field —
deserialization checks for whichever key is present and, for `[filament]`,
wraps the single table as a one-entry map under slot `"1"`.

## 6. Profile library & global config

Standard location (via the `dirs` crate): `~/.config/printcalc/`
```
config.toml
printers/*.toml
filaments/*.toml
```
`ref = "bambu-a1"` resolves to `printers/bambu-a1.toml`. WASM builds skip
this entirely (no filesystem) — the browser side supplies embedded profiles
directly (Full/Bundled jobs), or the host page manages its own profile
storage (e.g. localStorage) and passes profile JSON/TOML strings in.

## 7. CLI command surface

Git-style subcommands as the primary/scriptable interface; `--interactive`
prompts for any fields not passed as flags.

```
printcalc job new --slice widget.gcode.3mf --printer bambu-a1 --filament pla-black -o widget.toml
printcalc job new --slice project.gcode.3mf --plate all -o project.toml   # default; sums every plate
printcalc job new --slice project.gcode.3mf --plate 2 -o plate2.toml      # cost just plate 2
printcalc job new --slice vase_ams.gcode.3mf --printer bambu-a1 --filament-map 1=pla-black,2=pla-red,4=petg-clear -o vase.toml
printcalc job new --slice widget.gcode.3mf --interactive
printcalc job calc widget.toml [--format json|table]
printcalc job bundle widget.toml -o widget-bundled.toml   # re-tier Default/Full -> Bundled

printcalc printer add --name bambu-a1 --interactive
printcalc printer list
printcalc printer show bambu-a1

printcalc filament add --name pla-black --price-per-kg 18.99 --interactive
printcalc filament list
printcalc filament show pla-black
```

`--plate` is ignored (with a warning) for single-plate formats. It's stored
on the job (`[source] plate = ...`, see §5) so `job calc` doesn't need it
repeated.

`--filament` (one profile for the whole job) and `--filament-map`
(`slot=name` pairs, one per AMS tray/extruder) are mutually exclusive —
both just write the same underlying `filament` map (§2, §5), `--filament`
as sugar for a one-entry map under slot `"1"`. Single-material formats
(Prusa gcode/bgcode/sl1/sl1s) only ever accept `--filament`, since there's
exactly one slot (and it always normalizes to `"1"`, see §4). `job new
--interactive` parses the sliced file first and only prompts per-slot if it
actually finds more than one `FilamentUsage` with nonzero usage; a
single-material file still gets one plain filament prompt regardless of
printer type.

`job calc` output is a cost breakdown: material, wear, electricity, labor,
hardware line items + total, followed by suggested sale prices at each
configured markup percentage. Available in `table` (default, human-readable,
colored when stdout is a TTY) or `json` (scriptable) form.

**Table format: receipt style, dotted leaders.** Chosen over a plain
aligned-columns layout and a bordered/boxed table after comparing mockups —
most compact, and reads well even for a single line item. Each cost category
gets its own color (material=cyan, wear/electricity=amber, labor=rose,
hardware=blue); the total line and suggested prices are bold/green. Colors
auto-disable when stdout isn't a TTY (piped/redirected), and the dotted
alignment holds either way since it's plain text underneath. No
table-drawing dependency needed — hand-built strings plus a lightweight
color crate (e.g. `owo-colors`) gated on an `is_terminal` check.

```
$ printcalc job calc widget.toml

  Material.............. $4.23
  Printer wear.......... $0.67
  Electricity........... $0.27
  Labor.................. $3.75
  Hardware............... $0.48
  ─────────────────────────
  TOTAL................... $9.40

  sale price @ 50% / 60% / 70%:  $14.10  $15.04  $15.98
```

When `plate = "all"` resolves to more than one plate, a plate breakdown is
prepended using the same dotted-leader rhythm, just with a compound
`time · weight` value instead of a dollar figure. For a single-plate job
this block is omitted entirely and the receipt starts straight at
`Material`:

```
$ printcalc job calc project.toml

  Plate 1....................... 17m17s · 6.66g
  Plate 2....................... 16m28s · 5.15g
  ────────────────────────────────────────
  Combined...................... 33m45s · 11.81g

  Material.............. $0.25
  Printer wear.......... $0.04
  Electricity........... $0.02
  Labor.................. $2.50
  Hardware............... $0.00
  ─────────────────────────
  TOTAL................... $2.81

  sale price @ 50% / 60% / 70%:  $4.22  $4.50  $4.78
```
(using `pla-black` at $18.99/kg with a 1.1 efficiency factor, `bambu-a1`'s
wear/electricity rates from §3/§5, and 10 minutes of labor at $15/hr)

When a job's `filament` map has more than one entry and more than one
profile actually contributed cost (Bambu AMS / Prusa multi-extruder), each
plate's line breaks into one sub-line per slot instead of a single compound
value, and the `Material` line splits into one line per profile instead of
one combined line:

```
$ printcalc job calc vase.toml

  Plate 1....................... 22m40s
    pla-black................... 6.20g
    pla-red....................... 3.40g
    petg-clear...................... 1.85g

  Material (pla-black)........... $0.13
  Material (pla-red)............... $0.07
  Material (petg-clear).............. $0.05
  Printer wear.......... $0.03
  Electricity........... $0.01
  Labor.................. $3.75
  Hardware............... $0.00
  ─────────────────────────
  TOTAL................... $4.04

  sale price @ 50% / 60% / 70%:  $6.06  $6.46  $6.87
```
(using `pla-black`/`pla-red` at $18.99/kg and `petg-clear` at $22.99/kg, all
at a 1.1 efficiency factor, `bambu-a1`'s wear/electricity rates from §3/§5,
and 15 minutes of labor at $15/hr — a multi-slot job where only one profile
is actually in play, e.g. one color assigned to every tray, still collapses
back to a single `Material` line same as a plain single-filament job)

## 8. WASM target

`printcalc-wasm` wraps `printcalc_lib::ops` with `wasm-bindgen` — the same
functions `printcalc-cli` calls. Since there's no filesystem, the exposed API
works on in-memory bytes/strings only (no `ref =` library resolution; the
JS caller always passes embedded profile JSON/TOML):

```rust
#[wasm_bindgen]
pub fn parse_slice_file(bytes: &[u8], filename: &str) -> Result<JsValue, JsError>; // -> SliceSummary as JSON

#[wasm_bindgen]
pub fn calc_job(job_toml: &str) -> Result<JsValue, JsError>; // -> CostBreakdown as JSON
```
The JS host page is responsible for file input, profile persistence (e.g.
localStorage), and rendering results — the wasm module stays a pure
calculation/parsing library.

## 9. Build order (suggested milestones)

1. `printcalc-lib`: model types + TOML (de)serialization + cost calc, no
   parsers yet — unit-testable against hand-written SliceSummary values.
2. `prusa-sl1` parser (easiest — already have a real sample file to test
   against in `Testfiles/`).
3. `printcalc-cli` MVP: printer/filament add+list+show, job new/calc against
   the sl1 parser only.
4. `prusa-gcode` parser (text parsing, no zip handling needed) — write
   `prusa_metadata.rs`'s key=value parser here first.
5. `bambu-3mf` parser against `Testfiles/single_plate.gcode.3mf` and
   `multi_plate.gcode.3mf` — implement `plate_selection: Option<u32>`
   (`None`/`Some(n)`) and the per-plate `job calc` breakdown at the same
   time, since both demo files are on hand to test against. Populate
   `FilamentUsage` per `<filament id=...>` element while here; verify it
   against `single_plate_multicolor.gcode.3mf` /
   `multi_plate_multicolor.gcode.3mf` too (two AMS filaments per plate)
   so the single-filament and multi-filament cases are both covered
   before moving on.
6. `prusa-bgcode` parser against
   `Testfiles/Shape-Box_0.4n_0.2mm_PLA_MINIIS_24m.bgcode` — binary block
   framing (parse the `GCDE` header and walk blocks to find the metadata
   block) on top of the `prusa_metadata.rs` parser already written for
   `prusa-gcode`. Also add the comma-separated multi-extruder value
   parsing (see §4) to `prusa_metadata.rs` here, verified against
   `Shape-Box_0.4n_0.2mm_PLA_COREONEINDX_19m.gcode` (single-extruder) and
   `"Shape-Box_0.4n_0.2mm_PLA,PLA_COREONEINDX_19m.gcode"` (dual-extruder,
   same model) — both `.gcode` and `.bgcode` share this parser.
7. Multi-slot filament costing end-to-end: `[filaments]` TOML
   (de)serialization (plus the `[filament]` one-entry-map sugar), the
   `--filament-map` / per-slot `--interactive` prompting (§7), and the
   split `Material (<name>)` `job calc` output (§7) — the cost formula
   itself (§3) needs no separate work here since it's already written as
   a single per-slot loop from milestone 1 onward.
8. Job tiers: Full (embed profiles) and Bundled (embed SliceSummary,
   including its `plates` list) + `job bundle` re-tiering command.
9. `--interactive` prompts across `job new` / `printer add` / `filament add`.
10. `printcalc-wasm` crate + a minimal demo HTML page exercising it.
11. Polish `printcalc-lib`'s public API for external use: doc comments on all
    public items, a crate-level example in `lib.rs`, `README.md`, semver-
    conscious module layout, before publishing to crates.io.

## Open questions / later

- Currency is treated as a bare number/no symbol in v1; multi-currency
  support not planned unless needed.
