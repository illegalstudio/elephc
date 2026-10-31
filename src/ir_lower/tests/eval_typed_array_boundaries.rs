//! Purpose:
//! Refuses opaque eval normalization of typed indexed-array aliases still shared with native code.
//!
//! Called from:
//! - EIR regression CI for all supported targets.
//!
//! Key details:
//! - Parameters, captured aliases, properties, static properties and native foreach roots are covered.
//! - Boxed aliases and unrelated literal eval writes retain their supported behavior.

use crate::{codegen::platform::Target, ir_lower::LoweringError};
use std::path::Path;

/// Native typed-element aliases fail before eval can normalize their shared payload.
#[test]
fn opaque_eval_refuses_native_typed_indexed_aliases_on_every_target() {
    let cases = [
        ("parameter", r#"<?php
function nativeArrayParameter(array<int> &$items, string $source): void { eval($source); }
$items = [1]; nativeArrayParameter($items, $argc > 0 ? '$items[0] = 2;' : '');
"#),
        ("capture", r#"<?php
function nativeArrayCapture(array<int> &$items, string $source): void {
    $callback = function() use (&$items, $source): void { eval($source); }; $callback();
}
$items = [1]; nativeArrayCapture($items, $argc > 0 ? '$items[0] = 2;' : '');
"#),
        ("property", r#"<?php
class NativeArrayProperty { public array<int> $items = [1]; }
$holder = new NativeArrayProperty(); $items =& $holder->items;
$source = $argc > 0 ? '$items[0] = 2;' : ''; eval($source);
"#),
        ("static", r#"<?php
class NativeArrayStaticProperty { public static array<int> $items = [1]; }
$items =& NativeArrayStaticProperty::$items;
$source = $argc > 0 ? '$items[0] = 2;' : ''; eval($source);
"#),
        ("nullable_property", r#"<?php
class NativeNullableArrayProperty { public ?array<int> $items = null; }
$holder = new NativeNullableArrayProperty(); $holder->items = [1]; $items =& $holder->items;
$source = $argc > 0 ? '$items[0] = 2;' : ''; eval($source);
"#),
        ("foreach", r#"<?php
function nativeNestedArrayForeach(array<array<int>> &$rows, string $source): void {
    foreach ($rows as &$item) { eval($source); }
}
$rows = [[1]]; nativeNestedArrayForeach($rows, $argc > 0 ? '$item[0] = 2;' : '');
"#),
    ];
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        for (case, source) in cases {
            let error = match super::try_lower_source_at_for_target(
                source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
            ) {
                Err(LoweringError::Unsupported(error)) => error,
                other => panic!("{name}/{case}: expected native element-layout refusal, got {other:?}"),
            };
            assert!(error.message.contains("Unsupported eval: native reference")
                && error.message.contains("unboxed array elements"), "{name}/{case}: {}", error.message);
        }
    }
}

/// Boxed whole-local aliases and literal writes to other names do not acquire this refusal.
#[test]
fn eval_keeps_boxed_array_aliases_and_name_precise_native_scopes_on_every_target() {
    let source = r#"<?php
function namedLiteralNativeArray(array<int> &$items): void {
    $other = 1; eval('$other = 3;'); echo $other, $items[0];
}
function boxedLocalEvalArray(string $source): void {
    $items = [1, 2]; $alias =& $items; eval($source); echo $alias[0];
}
$items = [1]; namedLiteralNativeArray($items);
boxedLocalEvalArray($argc > 0 ? '$items[0] = 4;' : '');
"#;
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap(),
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
