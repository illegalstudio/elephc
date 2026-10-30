//! Purpose:
//! Test binary gathering every mbstring suite that leaves the engine's process-wide state alone,
//! one module per file, so a cold build compiles and links one executable instead of forty.
//!
//! Called from:
//! - `cargo test -p elephc-mbstring --test engine` (one suite: `--test engine <suite>::`).
//!
//! Key details:
//! - Suites that install the regex or MIME provider, configure the engine, or serialize
//!   against process-wide string identities are `[[test]]` targets of their own in Cargo.toml.
//! - `every_suite_file_is_registered` fails when a `tests/*.rs` file is in neither place.

#[path = "abi.rs"]
mod abi;
#[path = "arrays.rs"]
mod arrays;
#[path = "batches.rs"]
mod batches;
#[path = "capture_apply.rs"]
mod capture_apply;
#[path = "catalog.rs"]
mod catalog;
#[path = "check_abi.rs"]
mod check_abi;
#[path = "codecs.rs"]
mod codecs;
#[path = "coercion_abi.rs"]
mod coercion_abi;
#[path = "coercions.rs"]
mod coercions;
#[path = "conversion_abi.rs"]
mod conversion_abi;
#[path = "conversion_errors.rs"]
mod conversion_errors;
#[path = "detect.rs"]
mod detect;
#[path = "detect_many.rs"]
mod detect_many;
#[path = "detected_conversion.rs"]
mod detected_conversion;
#[path = "encoding_list_order.rs"]
mod encoding_list_order;
#[path = "entities.rs"]
mod entities;
#[path = "euctw.rs"]
mod euctw;
#[path = "gb18030.rs"]
mod gb18030;
#[path = "http_input.rs"]
mod http_input;
#[path = "hz.rs"]
mod hz;
#[path = "info.rs"]
mod info;
#[path = "ini.rs"]
mod ini;
#[path = "ini_native.rs"]
mod ini_native;
#[path = "iso2022kr.rs"]
mod iso2022kr;
#[path = "jis.rs"]
mod jis;
#[path = "kana.rs"]
mod kana;
#[path = "mime.rs"]
mod mime;
#[path = "mime_abi.rs"]
mod mime_abi;
#[path = "mobile_utf8.rs"]
mod mobile_utf8;
#[path = "operations.rs"]
mod operations;
#[path = "output_handler.rs"]
mod output_handler;
#[path = "parse_str.rs"]
mod parse_str;
#[path = "restore.rs"]
mod restore;
#[path = "snapshot.rs"]
mod snapshot;
#[path = "split_batches.rs"]
mod split_batches;
#[path = "state.rs"]
mod state;
#[path = "text.rs"]
mod text;
#[path = "transfer.rs"]
mod transfer;
#[path = "unicode.rs"]
mod unicode;
#[path = "utf7.rs"]
mod utf7;

/// Verifies every `tests/*.rs` suite is mounted here or declared as its own `[[test]]`
/// target: with `autotests = false`, a file in neither place would silently never run.
#[test]
fn every_suite_file_is_registered() {
    let tests_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let engine = include_str!("engine.rs");
    let manifest = include_str!("../Cargo.toml");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&tests_dir).expect("read tests directory") {
        let name = entry.expect("tests entry").file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".rs") else {
            continue;
        };
        if stem == "engine" {
            continue;
        }
        let mounted = engine.contains(&format!("#[path = \"{name}\"]\nmod {stem};\n"));
        let declared = manifest.lines().any(|line| line.trim() == format!("path = \"tests/{name}\""));
        if !mounted && !declared {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "suites registered nowhere: {missing:?}");
}
