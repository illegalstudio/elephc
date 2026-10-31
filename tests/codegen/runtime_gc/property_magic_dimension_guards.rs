//! Purpose:
//! Checks declared dimension guards in classes with magic property accessors.
//!
//! Called from:
//! - Executable runtime GC tests on CI.
//!
//! Key details:
//! - Accessible declared slots bypass magic hooks in AOT and opaque eval.
//! - XML capability variants use separate compilations and strict heap checks.

use crate::support::*;

/// Never-initialized declarations retain initialization, type and readonly guards with magic hooks.
#[test]
fn test_magic_property_dimension_guards_bundle() { verify(false); }

/// An XML-capable program retains the same ordinary declared-slot guards.
#[test]
fn test_magic_property_dimension_xml_guards_bundle() {
    if skip_without_xml_native("test_magic_property_dimension_xml_guards_bundle") { return; }
    verify(true);
}

/// Runs one identical PHP body natively and through opaque eval without accessor output.
fn verify(xml: bool) {
    let source = r#"<?php
class MagicFetchGet {
    private array $items;
    public function __get(string $name): mixed { echo 'unexpected-get|'; return null; }
    public function add(): array { $this->items[] = 3; return $this->items; }
}
class MagicFetchSet {
    private array $items;
    public function __set(string $name, mixed $value): void { echo 'unexpected-set|'; }
    public function add(): array { $this->items[] = 4; return $this->items; }
}
class MagicFetchBoth {
    private array $items;
    public function __get(string $name): mixed { echo 'unexpected-get|'; return null; }
    public function __set(string $name, mixed $value): void { echo 'unexpected-set|'; }
    public function add(): array { $this->items[0] = 5; return $this->items; }
}
class MagicFetchNull {
    public ?ArrayObject $items = null;
    public function __get(string $name): mixed { echo 'unexpected-get|'; return null; }
}
class MagicFetchSetNull {
    public ?ArrayObject $items = null;
    public function __set(string $name, mixed $value): void { echo 'unexpected-set|'; }
}
class MagicFetchReadonly {
    public readonly array $items;
    public function __get(string $name): mixed { echo 'unexpected-get|'; return null; }
    public function add(): void { $this->items[] = 1; }
}
class MagicFetchPrivateSet {
    public private(set) ?array $items = null;
    public function __get(string $name): mixed { echo 'unexpected-get|'; return null; }
    public function __set(string $name, mixed $value): void { echo 'unexpected-set|'; }
}
function magicFetchWrite(mixed $box, bool $append): void {
    try { if ($append) { $box->items[] = 1; } else { $box->items[0] = 1; } }
    catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
    echo isset($box->items) ? 'changed;' : 'empty;';
}
echo json_encode((new MagicFetchGet())->add()), '|';
echo json_encode((new MagicFetchSet())->add()), '|';
echo json_encode((new MagicFetchBoth())->add()), '|';
magicFetchWrite(new MagicFetchNull(), false);
magicFetchWrite(new MagicFetchNull(), true);
magicFetchWrite(new MagicFetchSetNull(), true);
try { (new MagicFetchReadonly())->add(); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|'; }
magicFetchWrite(new MagicFetchPrivateSet(), false);
magicFetchWrite(new MagicFetchPrivateSet(), true);
"#;
    let expected = concat!("[3]|[4]|[5]|",
        "TypeError:Cannot auto-initialize an array inside property MagicFetchNull::$items of type ?ArrayObject|empty;",
        "TypeError:Cannot auto-initialize an array inside property MagicFetchNull::$items of type ?ArrayObject|empty;",
        "TypeError:Cannot auto-initialize an array inside property MagicFetchSetNull::$items of type ?ArrayObject|empty;",
        "Error:Cannot indirectly modify readonly property MagicFetchReadonly::$items|",
        "Error:Cannot indirectly modify private(set) property MagicFetchPrivateSet::$items from global scope|empty;",
        "Error:Cannot indirectly modify private(set) property MagicFetchPrivateSet::$items from global scope|empty;");
    let capability = if xml { "function magicFetchXmlCapability(): void { simplexml_load_string('<r/>'); }" } else { "" };
    let native = format!("{source}\n{capability}");
    let renamed = source.replace("MagicFetch", "EvalMagicFetch").replace("magicFetch", "evalMagicFetch");
    let body = renamed.strip_prefix("<?php").unwrap().replace('\\', "\\\\").replace('\'', "\\'");
    let eval_capability = capability.replace("magicFetchXmlCapability", "evalMagicFetchXmlCapability");
    let eval = format!("<?php {eval_capability} $source = $argc > 0 ? '{body}' : ''; eval($source);");
    let eval_expected = expected.replace("MagicFetch", "EvalMagicFetch");
    assert_bundle_with(&[
        BundleCase { name: "magic_declaration_aot", source: &native, expected },
        BundleCase { name: "magic_declaration_eval", source: &eval, expected: &eval_expected },
    ], |source| {
        let out = compile_and_run_with_heap_debug(source);
        assert!(out.success, "stdout={} stderr={}", out.stdout, out.stderr);
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
        out.stdout
    });
}
