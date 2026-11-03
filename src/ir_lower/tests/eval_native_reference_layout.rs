//! Purpose:
//! Preserves native reference payload layouts when dynamic eval invalidates local type facts.
//!
//! Called from:
//! - AST-to-EIR regression tests in CI for every supported target.
//!
//! Key details:
//! - A shared string/int/float property cell cannot be widened to Mixed behind its aliases.

/// Native parameter hints remain physical entry contracts while eval reads use owned value copies.
#[test]
fn dynamic_eval_native_parameters_copy_current_values_on_every_target() {
    let source = r#"<?php
function nativeEvalValueView(int &$value, string $code): void {
    eval($code); echo gettype($value), ':', $value;
}
function nativeEvalCaptureView(int &$value, string $code): void {
    eval($code);
    $read = function () use (&$value): void { echo gettype($value); };
    $read();
}
function &nativeEvalReferenceReturnView(int &$value, string $code): int {
    eval($code); $value = 3; return $value;
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function|
            function.name == "nativeEvalValueView").unwrap();
        assert_eq!(function.params[0].php_type, crate::types::PhpType::Int, "{target}: retain entry ABI");
        let value = function.locals.iter().find(|local| local.name.as_deref() == Some("value")).unwrap();
        assert_eq!(value.php_type, crate::types::PhpType::Mixed, "{target}: detached locals need boxed storage");
        let reads = function.instructions.iter().filter(|inst|
            inst.op == crate::ir::Op::LoadRefCellValue).collect::<Vec<_>>();
        assert!(reads.len() >= 2, "{target}: both native consumers must read the actual PHP value");
        for read in reads {
            assert_eq!(read.result_php_type, crate::types::PhpType::Mixed, "{target}");
            assert_eq!(read.result_ownership, crate::ir::Ownership::Owned, "{target}: raw boxing and canonical cloning both own their result");
        }
        let capture = module.closures.iter().find(|closure| closure.params.iter()
            .any(|parameter| parameter.name == "value" && parameter.by_ref)).unwrap();
        assert_eq!(capture.params.iter().find(|parameter| parameter.name == "value").unwrap().php_type,
            crate::types::PhpType::Int, "{target}: the closure must keep the captured cell's physical ABI");
        let outer = module.functions.iter().find(|function|
            function.name == "nativeEvalCaptureView").unwrap();
        assert!(outer.instructions.iter().filter(|inst| inst.op == crate::ir::Op::LoadRefCellValue)
            .filter_map(|inst| inst.result).all(|value| outer.instructions.iter().any(|inst|
                inst.op == crate::ir::Op::Release && inst.operands.contains(&value))),
            "{target}: by-reference descriptors must retire unused owning value views");
        let reference = module.functions.iter().find(|function|
            function.name == "nativeEvalReferenceReturnView").unwrap();
        assert!(reference.instructions.iter().any(|inst| inst.op == crate::ir::Op::AcquireRefCell),
            "{target}: eval must not make a previously supported reference return unrepresentable");
        crate::ir::validate_module(&module).unwrap();
    }
}

/// Programs without eval retain concrete array and hash access through whole-local aliases.
#[test]
fn ordinary_array_aliases_keep_typed_reference_payloads_on_every_target() {
    let source = r#"<?php
function ordinaryArrayAliasLayout(bool $run): void {
    $items = [1, 2]; $alias =& $items; $alias[0] = 4;
    $mapping = ['key' => 1]; $named =& $mapping; $named['key'] = 5;
    $maybe = $run ? 1 : null; $number =& $maybe;
    echo $items[0], $mapping['key'], $number;
}
ordinaryArrayAliasLayout($argc > 0);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "ordinaryArrayAliasLayout").unwrap();
        for name in ["items", "alias", "mapping", "named"] {
            let local = function.locals.iter().find(|local| local.name.as_deref() == Some(name)).unwrap();
            assert!(matches!(local.php_type.codegen_repr(), crate::types::PhpType::Array(_)
                | crate::types::PhpType::AssocArray { .. }), "{target}: {name} lost its typed array access");
        }
        for name in ["maybe", "number"] {
            let local = function.locals.iter().find(|local| local.name.as_deref() == Some(name)).unwrap();
            assert_eq!(local.php_type.codegen_repr(), crate::types::PhpType::TaggedScalar,
                "{target}: {name} lost its nullable scalar payload");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// Every backend checks native object snapshots and can detach their explicit lease on unset.
#[test]
fn dynamic_eval_object_aliases_guard_tags_and_classes_on_every_target() {
    let source = r#"<?php
class EvalObjectAliasLayout {
    public stdClass $node;
    public function __construct() { $this->node = new stdClass(); }
    public function &nodeRef(): stdClass { return $this->node; }
}
function evalObjectAliasLayout(bool $run): void {
    $box = new EvalObjectAliasLayout();
    $node =& $box->nodeRef();
    $source = $run ? '$node = null;' : '';
    try { eval($source); } catch (TypeError $error) { echo get_class($box->node); }
}
evalObjectAliasLayout($argc > 0);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        for symbol in ["__elephc_eval_scope_native_type_v1", "eval_reload_invalid_heap_value",
            "__rt_mixed_instanceof", "__rt_reference_cell_release", "eval_unset_hidden_owner"] {
            assert!(assembly.contains(symbol), "{target}: missing native reload boundary {symbol}");
        }
    }
}

/// Whole local array aliases agree on boxed storage before eval can normalize their elements.
#[test]
fn dynamic_eval_array_aliases_use_boxed_reference_payloads_on_every_target() {
    let source = r#"<?php
function evalArrayAliasLayout(bool $run): void {
    $items = [1, 2];
    $alias =& $items;
    $source = $run ? '$items[0] = 4; $alias[] = 3;' : '';
    eval($source);
    echo $items[0], $alias[2];
}
evalArrayAliasLayout($argc > 0);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let function = module.functions.iter().find(|function| function.name == "evalArrayAliasLayout").unwrap();
        for name in ["items", "alias"] {
            let local = function.locals.iter().find(|local| local.name.as_deref() == Some(name)).unwrap();
            assert_eq!(local.php_type.codegen_repr(), crate::types::PhpType::Mixed,
                "{target}: {name} must retain the shared boxed payload shape");
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}

/// An opaque eval barrier preserves string and scalar cells shared with native properties.
#[test]
fn dynamic_eval_keeps_native_reference_payload_layouts_on_every_target() {
    let source = r#"<?php
class EvalReferenceLayout {
    public string $text = 'before';
    public int $number = 1;
    public float $weight = 1.5;
    public ?int $maybe = null;
    public function &textRef(): string { return $this->text; }
    public function &numberRef(): int { return $this->number; }
    public function &weightRef(): float { return $this->weight; }
    public function &maybeRef(): ?int { return $this->maybe; }
}
$box = new EvalReferenceLayout();
$text =& $box->textRef();
$number =& $box->numberRef();
$weight =& $box->weightRef();
$maybe =& $box->maybeRef();
$source = $argc > 0 ? '$box->text = "after"; $number = 2; $weight = 2.5; $maybe = 3;' : '';
eval($source);
echo $text, $number, $weight, $maybe;
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        for (name, ty) in [("text", crate::types::PhpType::Str),
            ("number", crate::types::PhpType::Int), ("weight", crate::types::PhpType::Float),
            ("maybe", crate::types::PhpType::Union(vec![crate::types::PhpType::Int, crate::types::PhpType::Void]).codegen_repr())] {
            let local = main.locals.iter().find(|local| local.name.as_deref() == Some(name)).unwrap();
            assert_eq!(local.php_type.codegen_repr(), ty, "{target}: {name} lost its shared cell layout");
            let owner = local.ref_cell_owner.expect("native alias needs an explicit lease for eval unset");
            assert_eq!(main.locals[owner.as_raw() as usize].kind, crate::ir::LocalKind::RefCell,
                "{target}: {name} must retire its own lease when eval detaches it");
        }
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(assembly.contains("__rt_reference_cell_release"), "{target}: missing detached lease retirement");
        assert!(assembly.contains("eval_reload_nullable_int_null"), "{target}: missing nullable scalar tag guard");
    }
}

/// A final native-alias unset preserves the established payload ABI for earlier eval barriers.
#[test]
fn dynamic_eval_unset_keeps_numeric_alias_payloads_in_the_sync_inventory_on_every_target() {
    let source = r#"<?php
class EvalUnsetReferenceLayout {
    public int $number = 1;
    public function &numberRef(): int { return $this->number; }
}
$box = new EvalUnsetReferenceLayout();
$number =& $box->numberRef();
$source = $argc > 0 ? '$number = 2;' : '';
eval($source);
echo $number;
unset($number, $box, $source);
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap();
        let number = main
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some("number"))
            .unwrap();
        assert_eq!(
            number.php_type.codegen_repr(),
            crate::types::PhpType::Int,
            "{target}: final unset must preserve the native reference payload ABI"
        );
        assert!(
            number.ref_cell_owner.is_some(),
            "{target}: the native alias must retain its explicit lease metadata"
        );
        crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
    }
}
