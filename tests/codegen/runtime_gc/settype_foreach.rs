//! Purpose:
//! Regression coverage for scalar settype updates of boxed hash foreach locals.
//!
//! Called from:
//! - The codegen runtime GC suite.
//!
//! Key details:
//! - Loop backedges must preserve boxed storage and retire replaced string owners.

use crate::support::*;

/// Static, referenced and global cells preserve converted runtime tags and shared writes.
#[test]
fn test_settype_foreach_mixed_storage_preserves_shared_cells() {
    let source = r#"<?php
function staticInteger(int $n): void {
    static $s = null;
    $s = $n;
    settype($s, 'float');
    echo gettype($s), ':', $s, ';';
}
function staticMixed(): void {
    static $s = null;
    settype($s, 'float');
    echo gettype($s), ':', $s, ';';
    $s = 1.5;
}
function referenced(array<string, int> $source): void {
    foreach ($source as $value) {
        $alias = &$value;
        settype($value, 'string');
        echo gettype($value), ':', $value, ',', gettype($alias), ':', $alias, ';';
        settype($value, 'float');
        echo gettype($value), ':', $value, ',', gettype($alias), ':', $alias, ';';
        unset($alias);
    }
}
function globalValue(): void {
    global $g;
    settype($g, 'string');
    settype($g, 'float');
    echo gettype($g), ':', $g, ';';
}
staticInteger(7); staticInteger(8); echo '|';
staticMixed(); staticMixed(); echo '|';
referenced(['a' => 7]); echo '|';
$g = 7; globalValue(); echo gettype($g), ':', $g, '|';
$kept = [];
foreach (['a' => 7] as $value) {
    for ($i = 0; $i < 2; $i++) {
        $kept[] = $value; settype($value, 'string'); settype($value, 'float');
    }
}
foreach ($kept as $item) { echo gettype($item), ':', $item, ';'; }
unset($g, $kept, $value, $item);
"#;
    for optimize in [false, true] {
        let out = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(out.success, "ir-opt={optimize}: {}", out.stderr);
        assert_eq!(out.stdout,
            "double:7;double:8;|double:0;double:1.5;|string:7,string:7;double:7,double:7;|double:7;double:7|integer:7;double:7;",
            "ir-opt={optimize}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir-opt={optimize}: {}", out.stderr);
    }
}

/// Repeated conversions retain boxed storage even after the logical value narrows.
#[test]
fn test_settype_foreach_mixed_storage_sequential_conversions() {
    let source = r#"<?php
function twice(array<string, int> $source): void {
    $kept = [];
    foreach ($source as $value) {
        for ($i = 0; $i < 2; $i++) {
            $kept[] = $value;
            settype($value, 'string');
            echo gettype($value), ':', $value, ';';
            settype($value, 'float');
        }
    }
    echo '|';
    foreach ($kept as $item) { echo gettype($item), ':', $item, ';'; }
    echo '|', $source['a'], '|';
    unset($kept, $item, $value);
}
function twiceWithoutBackedge(array<string, int> $source): void {
    $kept = [];
    foreach ($source as $value) {
        settype($value, 'float');
        settype($value, 'string');
        $kept[] = $value;
    }
    foreach ($kept as $item) { echo gettype($item), ':', $item, ';'; }
    unset($kept, $item, $value);
}
function twiceMixed(mixed $value): void {
    settype($value, 'string');
    settype($value, 'int');
    echo gettype($value), ':', $value;
}
twice(['a' => 7]);
twiceWithoutBackedge(['a' => 7, 'b' => 9]);
echo '|';
twiceMixed(str_repeat('4', $argc) . '2');
"#;
    for optimize in [false, true] {
        let out = if optimize { compile_and_run_with_heap_debug(source) }
            else { without_ir_opt(|| compile_and_run_with_heap_debug(source)) };
        assert!(out.success, "ir-opt={optimize}: {}", out.stderr);
        assert_eq!(out.stdout,
            "string:7;string:7;|integer:7;double:7;|7|string:7;string:9;|integer:42",
            "ir-opt={optimize}: {}", out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
            "ir-opt={optimize}: {}", out.stderr);
    }
}

/// Declared and inferred builders observe the original integer and converted string.
#[test]
fn test_settype_foreach_mixed_storage_preserves_backedge_values() {
    for builder in ["$kept = [];", "array<int> $kept = [];"] {
        let source = format!(r#"<?php
function collect(array<string, int> $source): void {{
    {builder}
    foreach ($source as $value) {{
        for ($i = 0; $i < 2; $i++) {{
            $kept[] = $value;
            echo settype(type: 'string', var: $value) ? 'T' : 'F';
        }}
    }}
    echo '|';
    foreach ($kept as $item) {{ echo gettype($item), ':', $item, ';'; }}
    echo '|', $source['a'];
    unset($kept, $item, $value);
}}
collect(['a' => 7, 'b' => 9]);
"#);
        for optimize in [false, true] {
            let out = if optimize { compile_and_run_with_heap_debug(&source) }
                else { without_ir_opt(|| compile_and_run_with_heap_debug(&source)) };
            assert!(out.success, "{builder}, ir-opt={optimize}: {}", out.stderr);
            assert_eq!(out.stdout, "TTTT|integer:7;string:7;integer:9;string:9;|7",
                "{builder}, ir-opt={optimize}: {}", out.stderr);
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
                "{builder}, ir-opt={optimize}: {}", out.stderr);
        }
    }
}

/// Every scalar target replaces boxed string storage without changing existing aliases.
#[test]
fn test_settype_foreach_mixed_storage_retires_string_owners() {
    for (target, expected) in [
        ("integer", "integer:42;"), ("double", "double:42;"),
        ("boolean", "boolean:1;"), ("string", "string:42;"),
    ] {
        let source = format!(r#"<?php
function convert(array<string, string> $source): void {{
    $kept = [];
    foreach ($source as $key => $value) {{
        for ($i = 0; $i < 2; $i++) {{
            $kept[] = $value;
            settype($value, '{target}');
        }}
        unset($source);
        break;
    }}
    unset($value);
    foreach ($kept as $item) {{ echo gettype($item), ':', $item, ';'; }}
    unset($kept, $item);
}}
convert(['a' => str_repeat('4', $argc) . '2']);
"#);
        for optimize in [false, true] {
            let out = if optimize { compile_and_run_with_heap_debug(&source) }
                else { without_ir_opt(|| compile_and_run_with_heap_debug(&source)) };
            assert!(out.success, "{target}, ir-opt={optimize}: {}", out.stderr);
            assert_eq!(out.stdout, format!("string:42;{expected}"),
                "{target}, ir-opt={optimize}: {}", out.stderr);
            assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"),
                "{target}, ir-opt={optimize}: {}", out.stderr);
        }
    }
}
