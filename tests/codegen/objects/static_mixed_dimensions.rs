//! Purpose:
//! Verifies indexed writes and incdec through boxed static property storage.
//!
//! Called from:
//! - The object codegen integration suite.
//!
//! Key details:
//! - AOT and opaque eval preserve independent PHP array aliases and clean up owners.

use crate::support::*;

/// Indexed static Mixed writes separate the cell before mutating its array payload.
#[test]
fn test_mixed_static_property_indexed_writes_preserve_cow_bundle() {
    let source = r#"<?php
class StaticMixedDimensionStore { public static mixed $items = null; }
StaticMixedDimensionStore::$items = [1, 3];
$copy = StaticMixedDimensionStore::$items;
StaticMixedDimensionStore::$items[0]++; --StaticMixedDimensionStore::$items[1];
StaticMixedDimensionStore::$items['mark'] = 'kept';
echo json_encode(StaticMixedDimensionStore::$items), ':', json_encode($copy), "\n";
StaticMixedDimensionStore::$items = null;
unset($copy);
"#;
    let renamed = source.replace("StaticMixedDimensionStore", "EvalStaticMixedDimensionStore");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let expected = "{\"0\":2,\"1\":2,\"mark\":\"kept\"}:[1,3]\n";
    assert_bundle_with(&[
        BundleCase { name: "mixed_static_write_aot", source, expected },
        BundleCase { name: "mixed_static_write_eval", source: &eval, expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert_eq!(out.stderr.matches("Warning:").count(), 0, "{}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
