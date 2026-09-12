//! Purpose:
//! Regression coverage for demand-lowering inherited builtin DateTime method bodies.
//!
//! Called from:
//! - `crate::ir_lower::tests` through Rust's test harness.
//!
//! Key details:
//! - Instantiating a user subclass must retain every inherited interface symbol its object table uses.

use super::lower_source;
use crate::ir_lower::builtin_datetime::class_is_or_descends_from_builtin_datetime;

/// A user override keeps its own body while inherited DateTime interface entries reach EIR.
#[test]
fn datetime_subclass_allocation_lowers_inherited_interface_methods() {
    let module = lower_source(
        r#"<?php
class AdaptedDate extends dAtEtImE {
    public function format(string $format): string { return $format . func_num_args(); }
}
new AdaptedDate();
"#,
    );
    let lowered = module
        .class_methods
        .iter()
        .map(|function| function.name.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert!(lowered.contains("AdaptedDate::format"));
    for inherited in [
        "DateTime::__construct",
        "DateTime::getTimestamp",
        "DateTime::getMicrosecond",
        "DateTime::getTimezone",
        "DateTime::getOffset",
    ] {
        assert!(
            lowered.contains(inherited),
            "missing inherited DateTime interface body {inherited}"
        );
    }
}

/// Canonical ancestry lookup terminates safely when malformed metadata contains a cycle.
#[test]
fn datetime_subclass_ancestry_rejects_cycles() {
    let mut module = lower_source("<?php class CycleRoot {} new CycleRoot();");
    module
        .class_infos
        .get_mut("CycleRoot")
        .expect("missing CycleRoot metadata")
        .parent = Some("\\cycleroot".to_string());

    assert!(!class_is_or_descends_from_builtin_datetime(
        &module,
        "CYCLEROOT"
    ));
}
