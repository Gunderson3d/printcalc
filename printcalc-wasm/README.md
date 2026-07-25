# printcalc-wasm

`wasm-bindgen` bindings over `printcalc_lib::ops` — see `SPEC.md` §8. Two
exports: `parse_slice_file(bytes, filename)` and `calc_job(job_toml)`, both
returning JSON strings (`JSON.parse()` them on the JS side). No filesystem:
`calc_job` needs a Bundled-tier job (embedded profiles + embedded slice
summary) since there's no library to resolve a `ref =` against and no
sliced file to re-parse a `path =` against.

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
