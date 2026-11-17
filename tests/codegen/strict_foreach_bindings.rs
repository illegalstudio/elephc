//! Purpose:
//! Exercises foreach callback publication and source cleanup through opaque eval.
//!
//! Called from:
//! - Native codegen regressions for default and strict local profiles.
//!
//! Key details:
//! - Both IR modes validate output, callback exceptions and heap accounting.

use crate::support::*;

/// Foreach callbacks and throwing cleanup preserve visible bindings and owned results.
#[test]
fn test_strict_foreach_binding_publication_and_retirement_bundle() {
    let body = include_str!("../fixtures/strict/foreach_binding_lifetimes.php").strip_prefix("<?php\n").unwrap();
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    let source = format!("<?php $code=$argc>0?'{quoted}':'';eval($code);gc_collect_cycles();");
    let expected = "Kafter|A9:after|Vold|B9:after|Told|Xold:after|Uafter|Y9:after|C|Qbefore|P|Zold:before|C|N|P|S5|LJ|64|I|R|";
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        for strict in [false, true] {
            let mut flags = vec![mode, "--heap-debug"];
            if strict { flags.push("--strict-locals"); }
            let output = StrictFixture::new(&source).capture(&flags);
            assert_eq!(String::from_utf8_lossy(&output.stdout), expected, "{flags:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("HEAP DEBUG: leak summary: clean"),
                "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}
