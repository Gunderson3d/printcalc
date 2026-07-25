# printcalc-lib

Calculates 3D print job costs — material, print time, printer wear,
electricity, labor, and extra hardware — from real sliced-file output
instead of manual entry. This is the core library crate: all parsing and
cost-calculation logic lives here, standalone and reusable by other Rust
projects. `printcalc-cli` and `printcalc-wasm` in this workspace are thin
frontends over it (see `../SPEC.md` for the full design).

## Supported formats

| Format | Feature |
|---|---|
| Bambu Studio `.gcode.3mf` | `bambu-3mf` |
| PrusaSlicer/OrcaSlicer `.gcode` | `prusa-gcode` |
| PrusaSlicer binary `.bgcode` | `prusa-bgcode` |
| PrusaSlicer resin `.sl1`/`.sl1s` | `prusa-sl1` |

Enable only what you need — `bambu`/`prusa` are umbrella features pulling
in every format for that vendor:

```sh
cargo add printcalc-lib --no-default-features --features prusa-sl1
```

`default = ["bambu", "prusa"]` builds every format.

## Quick start

See the crate-level doc example (`cargo doc --open -p printcalc-lib`, or
the top of `src/lib.rs`) for a runnable end-to-end example of the core
`calculate_cost` path. For a real application reading actual files,
[`ops`] is the entry point — it handles parsing, profile-library
resolution, and job (de)serialization together.

## Multi-plate / multi-filament

Bambu `.gcode.3mf` plates can be costed individually or combined, and any
format with more than one filament slot (Bambu AMS, Prusa XL/Core One
IDEX) is priced per slot against its own filament profile — see
`../SPEC.md` §2–§4 for the data model these map onto.

## No filesystem? No problem

`formats::parse_slice_file` and `model::cost::calculate_cost` do no I/O of
their own — pass bytes/data in, get a result out. `library`/`ops`'s
filesystem-backed functions (profile library, job load/save) are the only
parts that assume a real filesystem; `printcalc-wasm` skips them entirely
and calls the pure functions directly.
