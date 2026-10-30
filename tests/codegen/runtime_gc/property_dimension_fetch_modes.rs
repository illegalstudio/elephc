//! Purpose:
//! Verifies typed property initialization and access diagnostics across dimension fetch modes.
//!
//! Called from:
//! - The executable runtime GC suite.
//!
//! Key details:
//! - Full messages and unchanged null/uninitialized storage are observable assertions.
//! - Unsupported PHP type corruption is refused consistently by AOT and supported eval syntax.

use crate::support::*;

/// Write and compound fetches preserve declarations, access order and every temporary owner.
#[test]
fn test_property_dimension_fetch_modes_bundle() {
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool { echo 'unexpected-warning|'; return true; });
class GuardFetchNull { public ?ArrayObject $items = null; }
class GuardFetchInt { public ?int $items = null; }
class GuardFetchUninitialized { public ?ArrayObject $items; }
class GuardFetchUnion { public string|int $items; }
class GuardFetchReadonly {
    public readonly ?ArrayObject $items;
    public function __construct(bool $initialize) { if ($initialize) { $this->items = null; } }
}
class GuardFetchPrivate {
    public private(set) ?ArrayObject $items;
    public function __construct(bool $initialize) { if ($initialize) { $this->items = null; } }
}
function GuardFetchTry(mixed $box, int $mode): void {
    try {
        if ($mode === 0) { $box->items[0] = 'x'; }
        elseif ($mode === 1) { $box->items[0] += 1; }
        elseif ($mode === 2) { $box->items[0] .= 'x'; }
        elseif ($mode === 3) { $box->items[] = 'x'; }
        else { $box->items[0][1] = 'x'; }
    } catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    echo isset($box->items) ? 'changed;' : 'empty;';
}
for ($mode = 0; $mode < 5; $mode++) { GuardFetchTry(new GuardFetchNull(), $mode); }
GuardFetchTry(new GuardFetchUninitialized(), 0);
GuardFetchTry(new GuardFetchUninitialized(), 1);
GuardFetchTry(new GuardFetchUnion(), 0);
GuardFetchTry(new GuardFetchReadonly(true), 1);
GuardFetchTry(new GuardFetchPrivate(true), 1);
GuardFetchTry(new GuardFetchReadonly(false), 0);
GuardFetchTry(new GuardFetchPrivate(false), 0);
GuardFetchTry(new GuardFetchInt(), 1);
restore_error_handler();
"#;
    let auto = |class: &str, ty: &str| format!("TypeError:Cannot auto-initialize an array inside property {class}::$items of type {ty}|empty;");
    let readonly = "Error:Cannot indirectly modify readonly property GuardFetchReadonly::$items|empty;";
    let private = "Error:Cannot indirectly modify private(set) property GuardFetchPrivate::$items from global scope|empty;";
    let expected = [
        auto("GuardFetchNull", "?ArrayObject").repeat(5),
        auto("GuardFetchUninitialized", "?ArrayObject"),
        "Error:Typed property GuardFetchUninitialized::$items must not be accessed before initialization|empty;".into(),
        auto("GuardFetchUnion", "string|int"),
        readonly.into(), private.into(), readonly.into(), private.into(), auto("GuardFetchInt", "?int"),
    ].concat();
    let renamed = source.replace("GuardFetch", "EvalGuardFetch");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval = format!("<?php $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let eval_expected = expected.replace("GuardFetch", "EvalGuardFetch");
    assert_bundle_with(&[
        BundleCase { name: "property_fetch_modes_aot", source, expected: &expected },
        BundleCase { name: "property_fetch_modes_eval", source: &eval, expected: &eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}

/// Indexed increments and coalesce assignment reject null slots before an element warning.
#[test]
fn test_property_dimension_incdec_and_coalesce_guards() {
    let out = compile_and_run_with_heap_debug(r#"<?php
set_error_handler(function(int $level, string $message): bool { echo 'unexpected-warning|'; return true; });
class IncFetchNull { public ?ArrayObject $items = null; }
class IncFetchReadonly {
    public readonly ?ArrayObject $items;
    public function __construct() { $this->items = null; }
}
class IncFetchPrivate { public private(set) ?ArrayObject $items = null; }
function IncFetchTry(mixed $box, int $mode): void {
    try {
        if ($mode === 0) { $box->items[0]++; }
        elseif ($mode === 1) { $result = ++$box->items[0]; }
        elseif ($mode === 2) { --$box->items[0]; }
        else { $box->items[0] ??= 'x'; }
    } catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    echo isset($box->items) ? 'changed;' : 'empty;';
}
for ($mode = 0; $mode < 4; $mode++) { IncFetchTry(new IncFetchNull(), $mode); }
IncFetchTry(new IncFetchReadonly(), 0); IncFetchTry(new IncFetchReadonly(), 1);
IncFetchTry(new IncFetchPrivate(), 0); IncFetchTry(new IncFetchPrivate(), 1);
restore_error_handler();
"#);
    let expected = "TypeError:Cannot auto-initialize an array inside property IncFetchNull::$items of type ?ArrayObject|empty;".repeat(4)
        + &"Error:Cannot indirectly modify readonly property IncFetchReadonly::$items|empty;".repeat(2)
        + &"Error:Cannot indirectly modify private(set) property IncFetchPrivate::$items from global scope|empty;".repeat(2);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
