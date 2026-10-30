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
fn test_property_dimension_fetch_modes_bundle() { verify_fetch_modes(false); }

/// XML capability must preserve ordinary append diagnostics in both execution paths.
#[test]
fn test_property_dimension_xml_fetch_modes_bundle() {
    if skip_without_xml_native("test_property_dimension_xml_fetch_modes_bundle") { return; }
    verify_fetch_modes(true);
}

/// Compiles independent capability variants so XML detection cannot mask the ordinary path.
fn verify_fetch_modes(xml: bool) {
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool { echo 'unexpected-warning|'; return true; });
class GuardFetchNull { public ?ArrayObject $items = null; }
class GuardFetchInt { public ?int $items = null; }
class GuardFetchUninitialized { public ?ArrayObject $items; }
class GuardFetchUnion { public string|int $items; }
class GuardFetchRequiredObject { public ArrayObject $items; }
class GuardFetchRequiredString { public string $items; }
class GuardFetchRequiredInt { public int $items; }
class GuardFetchRequiredReadonly { public readonly ArrayObject $items; }
class GuardFetchRequiredPrivate { public private(set) ArrayObject $items; }
class GuardFetchPrivateArray { public private(set) ?array $items = null; }
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
GuardFetchTry(new GuardFetchReadonly(false), 1);
GuardFetchTry(new GuardFetchPrivate(false), 1);
GuardFetchTry(new GuardFetchInt(), 1);
foreach ([0, 3, 1] as $requiredMode) {
    GuardFetchTry(new GuardFetchRequiredObject(), $requiredMode);
    GuardFetchTry(new GuardFetchRequiredString(), $requiredMode);
    GuardFetchTry(new GuardFetchRequiredInt(), $requiredMode);
    GuardFetchTry(new GuardFetchRequiredReadonly(), $requiredMode);
    GuardFetchTry(new GuardFetchRequiredPrivate(), $requiredMode);
}
GuardFetchTry(new GuardFetchPrivateArray(), 0);
GuardFetchTry(new GuardFetchPrivateArray(), 3);
function GuardFetchConcrete(GuardFetchRequiredObject $box, bool $append): void {
    try { if ($append) { $box->items[] = 1; } else { $box->items[0] = 1; } }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
}
GuardFetchConcrete(new GuardFetchRequiredObject(), false);
GuardFetchConcrete(new GuardFetchRequiredObject(), true);
restore_error_handler();
"#;
    let auto = |class: &str, ty: &str| format!("TypeError:Cannot auto-initialize an array inside property {class}::$items of type {ty}|empty;");
    let readonly = "Error:Cannot indirectly modify readonly property GuardFetchReadonly::$items|empty;";
    let private = "Error:Cannot indirectly modify private(set) property GuardFetchPrivate::$items from global scope|empty;";
    let mut expected = [
        auto("GuardFetchNull", "?ArrayObject").repeat(5),
        auto("GuardFetchUninitialized", "?ArrayObject"),
        "Error:Typed property GuardFetchUninitialized::$items must not be accessed before initialization|empty;".into(),
        auto("GuardFetchUnion", "string|int"),
        readonly.into(), private.into(), readonly.into(), private.into(),
        "Error:Typed property GuardFetchReadonly::$items must not be accessed before initialization|empty;".into(),
        "Error:Typed property GuardFetchPrivate::$items must not be accessed before initialization|empty;".into(),
        auto("GuardFetchInt", "?int"),
    ].concat();
    let required = [
        ("GuardFetchRequiredObject", "ArrayObject"), ("GuardFetchRequiredString", "string"),
        ("GuardFetchRequiredInt", "int"), ("GuardFetchRequiredReadonly", "ArrayObject"),
        ("GuardFetchRequiredPrivate", "ArrayObject"),
    ];
    for mode in [0, 3, 1] {
        for (class, ty) in required {
            expected += &if mode == 1 {
                format!("Error:Typed property {class}::$items must not be accessed before initialization|empty;")
            } else if class == "GuardFetchRequiredReadonly" {
                format!("Error:Cannot indirectly modify readonly property {class}::$items|empty;")
            } else if class == "GuardFetchRequiredPrivate" {
                format!("Error:Cannot indirectly modify private(set) property {class}::$items from global scope|empty;")
            } else { auto(class, ty) };
        }
    }
    expected += &"Error:Cannot indirectly modify private(set) property GuardFetchPrivateArray::$items from global scope|empty;".repeat(2);
    expected += &"TypeError:Cannot auto-initialize an array inside property GuardFetchRequiredObject::$items of type ArrayObject|".repeat(2);
    let source = if xml {
        source.replace("<?php", "<?php function GuardFetchXmlCapability(): void { simplexml_load_string('<r/>'); }")
    } else { source.to_string() };
    let renamed = source.replace("GuardFetch", "EvalGuardFetch");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let capability = if xml { "function FetchModeNativeXmlCapability(): void { simplexml_load_string('<r/>'); }" } else { "" };
    let eval = format!("<?php {capability} $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let eval_expected = expected.replace("GuardFetch", "EvalGuardFetch");
    assert_bundle_with(&[
        BundleCase { name: "property_fetch_modes_aot", source: &source, expected: &expected },
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

/// Nested RW validation precedes the element read without XML capability.
#[test]
fn test_property_nested_fetch_modes_bundle() { verify_nested_fetch_modes(false); }

/// XML dispatch preserves the same RW errors and once-only operand evaluation.
#[test]
fn test_property_nested_xml_fetch_modes_bundle() {
    if skip_without_xml_native("test_property_nested_xml_fetch_modes_bundle") { return; }
    verify_nested_fetch_modes(true);
}

/// Checks compound statements, expression results, increments and attached array COW.
fn verify_nested_fetch_modes(xml: bool) {
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool { echo 'unexpected-warning|'; return true; });
class NestedFetchReadonly {
    public readonly ?ArrayObject $items;
    public function __construct(bool $initialize) { if ($initialize) { $this->items = null; } }
}
class NestedFetchPrivate {
    public private(set) ?ArrayObject $items;
    public function __construct(bool $initialize) { if ($initialize) { $this->items = null; } }
}
class NestedFetchRequired { public ArrayObject $items; }
class NestedFetchNullable { public ?ArrayObject $items = null; }
class NestedFetchArray { public array $items = [[2]]; }
function NestedFetchKey(int $which): int { echo 'k', $which, '|'; return 0; }
function NestedFetchRhs(): int { echo 'rhs|'; return 1; }
function NestedFetchTry(mixed $box, int $mode): void {
    try {
        if ($mode === 0) { $box->items[NestedFetchKey(1)][NestedFetchKey(2)] += NestedFetchRhs(); }
        elseif ($mode === 1) { $box->items[NestedFetchKey(1)][NestedFetchKey(2)]++; }
        elseif ($mode === 2) { $result = ++$box->items[NestedFetchKey(1)][NestedFetchKey(2)]; }
        else { $result = ($box->items[NestedFetchKey(1)][NestedFetchKey(2)] += NestedFetchRhs()); }
    } catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    echo isset($box->items) ? 'changed;' : 'empty;';
}
for ($mode = 0; $mode < 4; $mode++) {
    NestedFetchTry(new NestedFetchReadonly(true), $mode);
    NestedFetchTry(new NestedFetchPrivate(true), $mode);
    NestedFetchTry(new NestedFetchReadonly(false), $mode);
    NestedFetchTry(new NestedFetchPrivate(false), $mode);
    NestedFetchTry(new NestedFetchRequired(), $mode);
    NestedFetchTry(new NestedFetchNullable(), $mode);
}
$box = new NestedFetchArray(); $copy = $box->items;
$result = ($box->items[NestedFetchKey(1)][NestedFetchKey(2)] += NestedFetchRhs());
echo $result, ':', json_encode($box->items), ':', json_encode($copy), '|';
$result = $box->items[NestedFetchKey(1)][NestedFetchKey(2)]++;
echo $result, ':', json_encode($box->items), '|';
$result = --$box->items[NestedFetchKey(1)][NestedFetchKey(2)];
echo $result, ':', json_encode($box->items), '|';
unset($box, $copy, $result);
function NestedFetchReplace(mixed &$box, mixed $replacement): int { echo 'swap|'; $box = $replacement; return 1; }
$box = new NestedFetchArray(); $old = $box; $replacement = new NestedFetchArray();
$replacement->items = [[10]];
$box->items[NestedFetchKey(1)][NestedFetchKey(2)] += NestedFetchReplace($box, $replacement);
echo json_encode($old->items), ':', json_encode($replacement->items), '|';
unset($box, $old, $replacement);
function NestedFetchReceiver(mixed $box): mixed { echo 'receiver|'; return $box; }
try { NestedFetchReceiver(new NestedFetchReadonly(true))->items[NestedFetchKey(1)][NestedFetchKey(2)] += NestedFetchRhs(); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
unset($error);
restore_error_handler();
"#;
    let mut expected = String::new();
    for mode in 0..4 {
        let trace = if mode == 0 || mode == 3 { "k1|k2|rhs|" } else { "k1|k2|" };
        for error in [
            "Error:Cannot indirectly modify readonly property NestedFetchReadonly::$items",
            "Error:Cannot indirectly modify private(set) property NestedFetchPrivate::$items from global scope",
            "Error:Typed property NestedFetchReadonly::$items must not be accessed before initialization",
            "Error:Typed property NestedFetchPrivate::$items must not be accessed before initialization",
            "Error:Typed property NestedFetchRequired::$items must not be accessed before initialization",
            "TypeError:Cannot auto-initialize an array inside property NestedFetchNullable::$items of type ?ArrayObject",
        ] { expected += &format!("{trace}{error}|empty;"); }
    }
    expected += "k1|k2|rhs|3:[[3]]:[[2]]|k1|k2|3:[[4]]|k1|k2|3:[[3]]|k1|k2|swap|[[2]]:[[11]]|receiver|k1|k2|rhs|Error:Cannot indirectly modify readonly property NestedFetchReadonly::$items|";
    let source = if xml {
        source.replace("<?php", "<?php function NestedFetchXmlCapability(): void { simplexml_load_string('<r/>'); }")
    } else { source.to_string() };
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
