//! Purpose:
//! Pins the cost of invalidating escaped callable locals at every opaque instruction.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Counts names walked, not time: `ESCAPED_LOCAL_PROBES` records both the names the walk
//!   visited and the names a scan of the whole epoch map would have visited.

use crate::ir_lower::context::ESCAPED_LOCAL_PROBES;

/// A body that clears many callable locals and keeps one by-reference local walks only that local
/// at each opaque call, where filtering the whole epoch map walked every local ever cleared.
#[test]
fn escaped_callable_locals_are_found_from_the_smaller_side() {
    let mut body = String::from("function body(int $n) {\n    $kept = 0;\n    $alias = &$kept;\n");
    for index in 0..40 {
        body.push_str(&format!("    $f{index} = fn() => {index};\n    $f{index} = $n + {index};\n"));
    }
    for index in 0..40 {
        body.push_str(&format!("    echo strrev((string) ($n + {index}));\n"));
    }
    body.push_str("    return $alias;\n}\necho body($argc);\n");
    let source = format!("<?php\n{body}");
    ESCAPED_LOCAL_PROBES.with(|probes| probes.set((0, 0)));
    super::lower_source(&source);
    let (walked, full_scan) = ESCAPED_LOCAL_PROBES.with(std::cell::Cell::get);
    eprintln!("ESCAPED_LOCAL_PROBES walked={walked} full_scan={full_scan}");
    assert!(full_scan > 0, "the fixture must reach the invalidation with tracked locals");
    assert!(walked * 4 < full_scan, "walked {walked} names where a full scan walks {full_scan}");
}
