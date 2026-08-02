# printcalc-wasm

`wasm-bindgen` bindings over `printcalc`, mirroring its crate-root
API for everything that works without a filesystem:

- `parse_slice_bytes(bytes, filename)` → `SliceSummary` JSON
- `parse_job(job_toml)` → `Job` JSON
- `parse_printer(printer_toml)` → `PrinterProfile` JSON
- `parse_material(material_toml)` → `MaterialProfile` JSON
- `new_job(name, source_json, printer_json, materials_json, labor_json?, hardware_json?)` → `Job` JSON
- `calculate(job_json, defaults_json?)` → `CostBreakdown` JSON
- `calc_job(job_toml)` → `CostBreakdown` JSON (one-shot convenience)
- `builtin_printers()` / `builtin_filaments()` → preset JSON arrays

All return JSON strings (`JSON.parse()` them on the JS side); the lib's
`disk` helpers have no counterpart here. Custom profiles are passed as
TOML to `parse_printer`/`parse_material` and embedded into the job JSON
(`printer: { "profile": … }`, `material: { "1": { "profile": … } }`).
Anything costed in the browser needs embedded profiles (or built-in
preset names) and an embedded slice summary (`calc_job` requires a
Bundled-tier job for both), since there's no library to resolve a `ref =`
against and no sliced file to re-parse a `path =` against.

## Building the demo

```sh
rustup target add wasm32-unknown-unknown   # once
cargo install wasm-bindgen-cli --version <matches the wasm-bindgen version in Cargo.lock>

cargo build -p printcalc-wasm --target wasm32-unknown-unknown --release
wasm-bindgen ../target/wasm32-unknown-unknown/release/printcalc_wasm.wasm \
  --out-dir demo/pkg --target web

# serve demo/ over http:// (not file://) -- browsers block ES module
# imports and wasm fetch from file:// origins
python3 -m http.server -d demo 8000
```

Then open `http://localhost:8000/`. `demo/pkg/` is a generated build
artifact (git-ignored) — regenerate it after any change to this crate.
