//! Purpose:
//! Corpus tests for the push parser: a PHP-style event dispatcher replays the probe
//! scripts run against PHP 8.5.10 / libxml2 2.15.3 and compares the rendered event
//! stream, positions and error codes with the recorded outputs.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib parser`, with `ELEPHC_XML_LIBXML2_LIB_DIR` set for the
//!   replay tests (`cfg(elephc_xml_native)`).
//! - `cargo test -p elephc-xml --lib -- regen --ignored` regenerates the PHP probe scripts
//!   in `fixtures/` from the replay tables and re-verifies the fixtures with `php`
//!   (`regen`, which needs no libxml2).
//!
//! Key details:
//! - `harness` renders events exactly like the probe scripts' handlers printed them
//!   (php `var_export`/`json_encode` spellings), so fixture text is compared verbatim.
//! - `style` holds the per-probe line spellings shared by `harness` and `regen`, so the
//!   replay and the generated scripts cannot drift apart.

mod corpus_data;
#[cfg(elephc_xml_native)]
mod halt;
#[cfg(elephc_xml_native)]
mod harness;
#[cfg(elephc_xml_native)]
mod probes;
mod regen;
mod style;
