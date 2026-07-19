//! WASM bindings over `printcalc_lib::ops` for in-browser use. Thin
//! `wasm-bindgen` wrapper only — no business logic of its own.
//! See `SPEC.md` §1, §8.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn printcalc_wasm_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
