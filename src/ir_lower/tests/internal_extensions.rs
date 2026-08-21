//! Purpose:
//! Regression tests for DOM/libxml internal-extension lowering into typed EIR calls.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Constructors, methods, factories, functions, and virtual properties retain stable opcodes.

use std::collections::HashMap;

use crate::codegen::platform::{Arch, Platform, Target};
use crate::codegen::{
    generate_user_asm_from_ir, generate_user_asm_from_ir_with_options, Emit,
};
use crate::ir::{print_module, Immediate, Op};
use crate::types::PhpType;

use super::lower_source;

/// Verifies legacy DOM construction, mutation, serialization, and property reads bypass user methods.
#[test]
fn lowers_legacy_dom_operations_to_stable_internal_calls() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<root/>");
echo $document->version;
echo $document->saveXML();
"#,
    );
    let text = print_module(&module);
    assert!(module.required_runtime_features.dom_bridge);
    for expected in [
        "internal_extension#4303 flags=2",
        "internal_extension#4323 flags=1",
        "internal_extension#4577 flags=1",
        "internal_extension#4333 flags=1",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

/// Verifies manual internal constructors lower as null-returning calls on every target.
#[test]
fn lowers_manual_internal_constructor_calls_as_null_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$element = new DOMElement('old');
var_dump($element->__construct('new'));
$document = new DOMDocument();
var_dump($document->__construct('1.1', 'UTF-8'));
$fragment = new DOMDocumentFragment();
$root = new DOMElement('root');
var_dump($root->appendChild($fragment));
"#,
    );
    let text = print_module(&module);
    for opcode in ["internal_extension#4343 flags=1", "internal_extension#4303 flags=1"] {
        assert!(
            text.lines()
                .any(|line| line.contains(opcode) && line.contains("php=null")),
            "manual constructor omitted null result for {opcode}: {text}"
        );
    }
    assert!(
        text.lines().any(|line| {
            line.contains("internal_extension#4386 flags=3")
                && line.contains("php=DOMNode|false")
        }),
        "legacy appendChild omitted its wrapper-or-false result contract: {text}"
    );

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target:?} manual constructors failed: {error}"));
    }
}

/// Verifies modern static factories and companion libxml functions use the common extension opcode.
#[test]
fn lowers_modern_factory_and_libxml_function_to_internal_calls() {
    let module = lower_source(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
libxml_clear_errors();
$error = libxml_get_last_error();
echo $document->saveXML();
"#,
    );
    let text = print_module(&module);
    assert!(module.required_runtime_features.dom_bridge);
    for expected in [
        "internal_extension#4271 flags=2",
        "internal_extension#4098 flags=0",
        "internal_extension#4102 flags=4",
        "internal_extension#4275 flags=1",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
}

/// Verifies inherited SimpleXML native methods bypass empty userland vtable slots on x86_64.
#[test]
fn lowers_simplexml_descendant_method_to_bridge_on_linux_x86_64() {
    let mut module = lower_source(
        r#"<?php
class NativeNameXml extends SimpleXMLElement {
    public function nativeName(): string {
        return $this->getName();
    }
}

class OverrideNameXml extends SimpleXMLElement {
    public function getName(): string {
        return 'override';
    }
}

function overriddenName(OverrideNameXml $xml): string {
    return $xml->getName();
}
"#,
    );
    let text = print_module(&module);
    assert_eq!(
        text.matches("internal_extension#4437 flags=1").count(),
        1,
        "only inherited getName() should lower to its locked bridge opcode: {text}"
    );

    module.target = Target::new(Platform::Linux, Arch::X86_64);
    let assembly = generate_user_asm_from_ir(&module, false, false)
        .expect("inherited SimpleXML getName should lower for Linux x86_64");
    for expected in [
        "mov rcx, 4437",
        "mov DWORD PTR [r10 + 8], ecx",
        "call elephc_dom_call",
    ] {
        assert!(
            assembly.contains(expected),
            "Linux x86_64 inherited getName bridge omitted {expected}: {assembly}"
        );
    }
}

/// Verifies `parent::count()` has a concrete base method symbol backed by the native handler.
#[test]
fn lowers_simplexml_count_runtime_entry_for_parent_override() {
    let module = lower_source(
        r#"<?php
class CountingXml extends SimpleXMLElement {
    public function count(): int {
        return parent::count();
    }
}

$xml = new CountingXml('<root><child/><child/></root>');
echo count($xml);
"#,
    );
    assert!(
        super::super::program::class_method_already_lowered(
            &module,
            "SimpleXMLElement",
            "count",
            false,
        ),
        "SimpleXMLElement::count() runtime ABI entry was not materialized"
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4433 flags=1"),
        "SimpleXMLElement::count() runtime entry omitted the native handler: {text}"
    );
}

/// Verifies XPath array failure arms throw before typed count on every supported target.
#[test]
fn lowers_fallible_simplexml_xpath_count_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<root id="1"><child/></root>');
echo count($xml->xpath('/root/child'));
echo count($xml->xpath('//*['));
echo count($xml->attributes()->xpath('.'));
"#,
    );
    let text = print_module(&module);
    for expected in [
        "count.fallible_array.false",
        "count.fallible_array.null",
        "strict_eq",
        "is_null",
        "runtime.count",
        "throw",
    ] {
        assert!(
            text.contains(expected),
            "fallible XPath count omitted {expected}: {text}"
        );
    }

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("fallible XPath count should lower on every target");
        for expected in [
            "count(): Argument #1 ($value) must be of type Countable|array, false given",
            "count(): Argument #1 ($value) must be of type Countable|array, null given",
            "__rt_mixed_count",
        ] {
            assert!(
                assembly.contains(expected),
                "{target:?} fallible XPath count omitted {expected}: {assembly}"
            );
        }
    }
}

/// Verifies SimpleXML gap warnings decorate one detail on every supported target.
#[test]
fn lowers_simplexml_callsite_warning_context_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<r><a/><a/></r>');
$xml->a[3] = 'three';
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4457 flags=1") && text.contains("span: 3:1"),
        "numeric SimpleXML write must retain its source-backed handler call: {text}"
    );
    module.source_path = Some("main.php".to_string());

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("SimpleXML warning context should lower on every target");
        assert!(
            assembly.contains("Warning: main(): "),
            "{target:?} omitted the PHP callable warning prefix: {assembly}"
        );
        assert!(
            assembly.contains(" in main.php on line 3"),
            "{target:?} omitted the PHP source warning suffix: {assembly}"
        );
        let concat_call = if target.arch == Arch::X86_64 {
            "call __rt_concat"
        } else {
            "bl __rt_concat"
        };
        assert!(
            assembly.matches(concat_call).count() >= 2,
            "{target:?} omitted call-site diagnostic composition"
        );
    }
}

/// Verifies SimpleXML XPath warnings add only file and line on every supported target.
#[test]
fn lowers_simplexml_xpath_warning_location_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<r/>');
var_dump($xml->xpath('***'));
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4446 flags=1") && text.contains("span: 3:"),
        "SimpleXML XPath must retain its source-backed method call: {text}"
    );
    module.source_path = Some("main.php".to_string());

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("SimpleXML XPath warning location should lower on every target");
        assert!(
            assembly.contains(" in main.php on line 3"),
            "{target:?} omitted the PHP XPath source warning suffix: {assembly}"
        );
        let (flag_check, concat_call) = if target.arch == Arch::X86_64 {
            ("cmp ecx, 2", "call __rt_concat")
        } else {
            ("cmp w13, #2", "bl __rt_concat")
        };
        assert!(
            assembly.contains(flag_check),
            "{target:?} omitted the location-only diagnostic flag check"
        );
        assert!(
            assembly.matches(concat_call).count() >= 2,
            "{target:?} omitted XPath call-site diagnostic composition"
        );
    }
}

/// Verifies SimpleXML mutator warnings add only file and line on every supported target.
#[test]
fn lowers_simplexml_mutator_warning_locations_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<r id="1"/>');
$xml->addAttribute('id', '2');
$attributes = $xml->attributes();
$attributes->addChild('child');
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4428 flags=1") && text.contains("span: 3:"),
        "SimpleXML addAttribute must retain its source-backed method call: {text}"
    );
    assert!(
        text.contains("internal_extension#4429 flags=3") && text.contains("span: 5:"),
        "SimpleXML addChild must retain its source-backed method call: {text}"
    );
    module.source_path = Some("main.php".to_string());

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("SimpleXML mutator warning locations should lower on every target");
        for line in [3, 5] {
            assert!(
                assembly.contains(&format!(" in main.php on line {line}")),
                "{target:?} omitted a PHP mutator warning suffix: {assembly}"
            );
        }
        let (flag_check, concat_call) = if target.arch == Arch::X86_64 {
            ("cmp ecx, 2", "call __rt_concat")
        } else {
            ("cmp w13, #2", "bl __rt_concat")
        };
        assert!(
            assembly.contains(flag_check),
            "{target:?} omitted the mutator location-only diagnostic flag check"
        );
        assert!(
            assembly.matches(concat_call).count() >= 4,
            "{target:?} omitted mutator call-site diagnostic composition"
        );
    }
}

/// Verifies recursive SimpleXML result trees prevalidate and materialize on every target.
#[test]
fn lowers_recursive_simplexml_debug_result_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<r id="7"><a>A</a><a>B</a></r>');
if ($xml === false) {
    exit(2);
}
$debug = $xml->__debugInfo();
echo count($debug);
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4426 flags=1"),
        "SimpleXML::__debugInfo lost its locked bridge opcode: {text}"
    );

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("recursive SimpleXML debug results should lower on every target");
        let validator = target.extern_symbol("elephc_dom_validate_result_map_tree");
        let validator_call = match target.arch {
            Arch::AArch64 => format!("bl {validator}"),
            Arch::X86_64 => format!("call {validator}"),
        };
        for expected in [
            &validator_call,
            "dom_result_tree_value",
            "dom_result_tree_array_loop",
            "dom_result_tree_map_loop",
            "dom_result_tree_object",
            "dom_result_tree_value_object_callback",
            "__rt_array_push_refcounted",
            "__rt_hash_set",
        ] {
            assert!(
                assembly.contains(expected),
                "{target:?} recursive SimpleXML result assembly omitted {expected}"
            );
        }
        assert!(
            assembly
                .find(&validator_call)
                .zip(assembly.find("__rt_hash_new"))
                .is_some_and(|(validator, allocation)| validator < allocation),
            "{target:?} must prevalidate the complete result tree before PHP allocation"
        );
        if target.arch == Arch::X86_64 {
            for expected in [
                "mov QWORD PTR [rbp - 88], r12",
                "mov QWORD PTR [rbp - 96], r13",
                "mov r12, QWORD PTR [rbp - 88]",
                "mov r13, QWORD PTR [rbp - 96]",
            ] {
                assert!(
                    assembly.contains(expected),
                    "Linux x86_64 recursive materialization omitted ABI preservation: {expected}"
                );
            }
        }
    }
}

/// Verifies a property-dimension write on a fallible loader uses both SimpleXML handlers.
#[test]
fn lowers_fallible_simplexml_property_dimension_write_to_native_handlers() {
    let module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<people/>');
$xml->person['name'] = 'John';
"#,
    );
    let text = print_module(&module);
    for opcode in [4454, 4457] {
        assert!(
            module
                .functions
                .iter()
                .flat_map(|function| function.instructions.iter())
                .any(|instruction| {
                    instruction.op == Op::InternalExtensionCall
                        && matches!(
                            instruction.immediate,
                            Some(Immediate::InternalExtension {
                                opcode: actual,
                                ..
                            }) if actual == opcode
                        )
                }),
            "SimpleXML property-dimension write omitted opcode {opcode}: {text}"
        );
    }
    assert!(
        !text
            .lines()
            .any(|line| line.contains("runtime_call") && line.contains("span: 3:1")),
        "SimpleXML property-dimension write leaked to the generic runtime fallback: {text}"
    );
}

/// Verifies a nested SimpleXML write fetches its numeric parent with `BP_VAR_W`
/// before routing the named leaf through the native write handler.
#[test]
fn lowers_nested_simplexml_dimension_write_with_bp_var_w() {
    let module = lower_source(
        r#"<?php
$people = simplexml_load_string('<people><person/><person/></people>');
$people->person[3]['gender'] = 'male';
"#,
    );
    let text = print_module(&module);
    let instructions = module
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .collect::<Vec<_>>();
    let read = instructions
        .iter()
        .copied()
        .find(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension { opcode: 4453, .. })
                )
        })
        .unwrap_or_else(|| panic!("nested SimpleXML write omitted read_dimension: {text}"));
    assert_eq!(read.operands.len(), 3, "read_dimension lost its BP_VAR operand");
    let access_mode = read.operands[2];
    assert!(
        instructions.iter().any(|instruction| {
            instruction.result == Some(access_mode)
                && instruction.op == Op::ConstI64
                && instruction.immediate == Some(Immediate::I64(1))
        }),
        "nested SimpleXML read_dimension did not carry BP_VAR_W=1: {text}"
    );
    assert!(
        instructions.iter().any(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension { opcode: 4457, .. })
                )
        }),
        "nested SimpleXML write omitted write_dimension: {text}"
    );
    assert!(
        !text
            .lines()
            .any(|line| line.contains("runtime_call") && line.contains("span: 3:1")),
        "nested SimpleXML write leaked to the generic array fallback: {text}"
    );
}

/// Verifies PHP's syntactic `[]` marker remains distinct from a literal null offset in EIR.
#[test]
fn lowers_simplexml_append_dimension_with_dedicated_bridge_flag() {
    let module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<root/>');
$xml->bla->posts[]->name = 'FooBar';
"#,
    );
    let text = print_module(&module);
    let read = module
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .find(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension {
                        opcode: 4453,
                        flags: 11,
                    })
                )
        });
    assert!(
        read.is_some(),
        "SimpleXML append dimension must retain receiver, wrapper, and append flags: {text}"
    );
    assert!(
        !text.contains("internal_extension#4453 flags=3"),
        "the append dimension was lowered as an ordinary dimension read: {text}"
    );
    let instructions = module
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .collect::<Vec<_>>();
    let property_reads = instructions
        .iter()
        .copied()
        .filter(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension { opcode: 4454, .. })
                )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        property_reads.len(),
        2,
        "the nested write must fetch both SimpleXML properties: {text}"
    );
    for (position, property_read) in property_reads.into_iter().enumerate() {
        let property_address = property_read.operands[3];
        assert!(
            instructions.iter().any(|instruction| {
                instruction.result == Some(property_address)
                    && instruction.op == Op::ConstBool
                    && instruction.immediate == Some(Immediate::Bool(true))
            }),
            "the nested property read must request an addressable child: {text}"
        );
        let append_target = property_read.operands[4];
        let expected_append_target = position == 1;
        assert!(
            instructions.iter().any(|instruction| {
                instruction.result == Some(append_target)
                    && instruction.op == Op::ConstBool
                    && instruction.immediate == Some(Immediate::Bool(expected_append_target))
            }),
            "property read {position} must mark only the terminal append property: {text}"
        );
    }
}

/// Verifies native and inherited SimpleXML `current()` results stay typed for dimension reads.
#[test]
fn lowers_simplexml_foreach_current_to_declared_wrapper_type() {
    let mut module = lower_source(
        r#"<?php
class ChildXml extends SimpleXMLElement {}

$base = simplexml_load_string('<r><file glob="base"/></r>');
if ($base === false) { exit(2); }
foreach ($base->file as $file) {
    echo (string) $file['glob'];
}

$child = new ChildXml('<r><file glob="child"/></r>');
foreach ($child->file as $file) {
    echo (string) $file['glob'];
}
"#,
    );
    let text = print_module(&module);
    assert_eq!(
        text.matches("internal_extension#4453 flags=3").count(),
        2,
        "SimpleXML foreach dimension reads must retain their native handler: {text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| {
                line.contains("iter_current_value")
                    && line.contains("php=SimpleXMLElement")
            })
            .count(),
        2,
        "native and inherited current() results must use their declared wrapper type: {text}"
    );
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        generate_user_asm_from_ir(&module, false, false)
            .expect("typed SimpleXML foreach should lower on every supported target");
    }
}

/// Verifies a userland `current(): mixed` override is not narrowed to SimpleXML by foreach.
#[test]
fn preserves_simplexml_foreach_userland_current_override_type() {
    let mut module = lower_source(
        r#"<?php
class ScalarCurrentXml extends SimpleXMLElement {
    #[\ReturnTypeWillChange]
    public function current(): mixed {
        return 42;
    }
}

$xml = new ScalarCurrentXml('<r><file glob="value"/></r>');
foreach ($xml as $value) {
    echo $value['glob'];
}
"#,
    );
    let text = print_module(&module);
    assert!(
        text.lines().any(|line| {
            line.contains("iter_current_value") && line.contains("php=mixed")
        }),
        "the effective userland current() signature must remain Mixed: {text}"
    );
    assert!(
        !text.contains("internal_extension#4453 flags=3"),
        "a Mixed userland current() result must not use the SimpleXML dimension handler: {text}"
    );
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        generate_user_asm_from_ir(&module, false, false)
            .expect("Mixed SimpleXML foreach override should lower on every supported target");
    }
}

/// Verifies boxed parameter and foreach receivers keep dynamic SimpleXML property, dimension, and count dispatch.
#[test]
fn lowers_dynamic_mixed_simplexml_handlers_on_every_target() {
    let mut module = lower_source(
        r#"<?php
function inspect($xml): void {
    foreach ($xml->children() as $person) {
        echo (string) $person['name'];
        echo count($person);
    }
    for ($i = 0; $i < count($xml->person); $i++) {
        echo (string) $xml->person[$i]['name'];
    }
}

inspect(simplexml_load_string('<people><person name="Joe"/><person name="Boe"/></people>'));
"#,
    );
    let text = print_module(&module);
    assert!(
        module.required_runtime_features.dom_bridge,
        "dynamic SimpleXML handler dispatch must retain the DOM bridge: {text}"
    );
    let instructions = module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .flat_map(|function| function.instructions.iter())
        .collect::<Vec<_>>();
    assert!(
        instructions
            .iter()
            .any(|instruction| instruction.op == Op::PropGet && instruction.operands.len() == 4),
        "dynamic SimpleXML property reads lost their native handler operands: {text}"
    );
    assert!(
        instructions.iter().any(|instruction| {
            instruction.op == Op::RuntimeCall
                && instruction.immediate.is_none()
                && instruction.operands.len() == 4
        }),
        "dynamic SimpleXML dimension reads lost their access-mode operand: {text}"
    );

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("dynamic SimpleXML handlers should lower on every target");
        for expected in [
            "mixed_prop_simplexml_",
            "mixed_dimension_",
            "mixed_count_",
            "4454",
            "4453",
            "4449",
        ] {
            assert!(
                assembly.contains(expected),
                "{target:?} dynamic SimpleXML dispatch omitted {expected}: {assembly}"
            );
        }
        let wrapper_marker =
            "interface return wrapper SimpleXMLElement implements Iterator::current";
        let wrapper_start = assembly.find(wrapper_marker).unwrap_or_else(|| {
            panic!("{target:?} omitted the SimpleXML Iterator::current return wrapper")
        });
        let wrapper_body = &assembly[wrapper_start..];
        let wrapper_end = wrapper_body
            .find("\n    ret")
            .expect("SimpleXML Iterator::current return wrapper must terminate");
        let wrapper_body = &wrapper_body[..wrapper_end];
        let owned_release = match target.arch {
            Arch::AArch64 => "bl __rt_decref_object",
            Arch::X86_64 => "call __rt_decref_object",
        };
        assert!(
            wrapper_body.contains(owned_release),
            "{target:?} did not transfer the owned current() object into its Mixed wrapper: {wrapper_body}"
        );
    }
}

/// Verifies bug55098's untyped callback routes every SimpleXML object-handler operation
/// through runtime class guards, preserving the generic Mixed fallback on all targets.
#[test]
fn lowers_bug55098_untyped_callbacks_through_guarded_simplexml_handlers_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$xml = simplexml_load_string('<root id="before"><a><b>1</b><b>2</b><b>3</b></a></root>');
$nodes = $xml->a->b;
$callback = function ($n): void {
    $n->asXml();
    $n->attributes();
    $n->children();
    $n->getNamespaces();
    $n->xpath('/root/a/b');
    $n->addAttribute('attr', 'value');
    $n->addChild('child', 'value');
    $n->status = 'active';
    $n->outer[]->inner = 'foo';
    (string) $n['attr'];
    (bool) $n->outer;
    (bool) $n;
    count($n);
    (array) $n;
    (object) $n;
    isset($n->outer);
    isset($n['attr']);
    unset($n->outer);
    unset($n['attr']);
    unset($n->child);
};
$callback($nodes);
"#,
    );
    let text = print_module(&module);
    assert!(
        module.required_runtime_features.dom_bridge,
        "bug55098's dynamic callback must retain the DOM bridge: {text}"
    );
    let callback_instructions = module
        .functions
        .iter()
        .find(|function| function.name.starts_with("__eir_closure_main_"))
        .map(|function| function.instructions.iter().collect::<Vec<_>>())
        .expect("bug55098 fixture must lower its untyped callback as a closure function");
    assert!(
        callback_instructions
            .iter()
            .any(|instruction| instruction.op == Op::PropGet && instruction.operands.len() >= 4),
        "dynamic callback property reads lost guarded receiver metadata: {text}"
    );
    assert!(
        callback_instructions.iter().any(|instruction| {
            instruction.op == Op::RuntimeCall
                && instruction.immediate.is_none()
                && instruction.operands.len() >= 4
        }),
        "dynamic callback dimension reads lost guarded receiver metadata: {text}"
    );
    assert!(
        !callback_instructions.iter().any(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension { opcode: 4451..=4458, .. })
                )
        }),
        "untyped callback operations must not bypass Mixed guards with static handlers: {text}"
    );

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("bug55098 callback handlers should lower on every supported target");
        for expected in [
            "mixed_prop_simplexml_",
            "mixed_dimension_",
            "mixed_method_",
            "mixed_count_",
            "4451",
            "4452",
            "4453",
            "4454",
            "4455",
            "4456",
            "4457",
            "4458",
            "4447",
        ] {
            assert!(
                assembly.contains(expected),
                "{target:?} bug55098 guarded dispatch omitted {expected}: {assembly}"
            );
        }
    }
}

/// Verifies fallible SimpleXML comparisons guard false arms before native marshalling.
#[test]
fn lowers_fallible_simplexml_comparisons_on_every_target() {
    let mut module = lower_source(
        r#"<?php
class CompareA extends SimpleXMLElement {}
class CompareB extends SimpleXMLElement {}
$left = simplexml_load_string('<root/>');
$right = simplexml_load_string('<root/>');
var_dump($left == $right, $left != $right, $left <=> $right);
var_dump($left == false, false == $right, $left <=> false, false <=> $right);
$a = new CompareA('<root/>');
$b = new CompareB('<root/>');
var_dump($a == $b);
"#,
    );
    let text = print_module(&module);
    for expected in [
        "simplexml.compare.failure",
        "simplexml.compare.object",
        "internal_extension#4448 flags=1",
        "internal_extension#4447 flags=1",
    ] {
        assert!(
            text.contains(expected),
            "fallible SimpleXML comparison omitted {expected}: {text}"
        );
    }
    assert_eq!(
        text.matches("internal_extension#4448 flags=1").count(),
        4,
        "SimpleXML subclasses must share the native comparison handler: {text}"
    );
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("fallible SimpleXML comparison should lower on every target");
        for expected in ["4448", "4447", "dom_request_nullable_wrapper"] {
            assert!(
                assembly.contains(expected),
                "{target:?} fallible comparison omitted {expected}: {assembly}"
            );
        }
    }
}

/// Verifies SimpleXML object casts are identities and array casts use handler 4447 kind 5.
#[test]
fn lowers_simplexml_object_and_array_casts_without_generic_backend_casts() {
    let module = lower_source(
        r#"<?php
$foo = simplexml_load_string('<foo><bar><p>one</p><p>two</p><p>three</p></bar></foo>');
var_dump((object) $foo);
$p = $foo->bar->p;
$p = (array) $foo->bar->p;
echo count($p);
"#,
    );
    let text = print_module(&module);
    let cast_calls = module
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .filter(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension {
                        opcode: 4447,
                        flags: 1,
                    })
                )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cast_calls.len(),
        1,
        "only the array cast should invoke the SimpleXML cast handler: {text}"
    );
    assert_eq!(
        cast_calls[0].result_php_type,
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        "SimpleXML array cast lost its mixed-key recursive property result: {text}"
    );
    assert!(
        !module
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .any(|instruction| instruction.op == Op::Cast),
        "SimpleXML object/array casts leaked to the unsupported generic backend cast: {text}"
    );
}

/// Verifies `simplexml_import_dom()` releases an owned DOM wrapper read after narrowing.
#[test]
fn simplexml_import_dom_releases_narrowed_argument_temporary() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$element = $document->documentElement;
if ($element === null) { exit(2); }
$xml = simplexml_import_dom($element);
unset($xml, $element, $document);
"#,
    );
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("expected main EIR function");
    let argument = function
        .instructions
        .iter()
        .find(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension { opcode: 4106, .. })
                )
        })
        .and_then(|instruction| instruction.operands.first().copied())
        .expect("expected the DOM wrapper argument passed to simplexml_import_dom");
    assert!(
        function.instructions.iter().any(|instruction| {
            instruction.op == Op::Release
                && instruction.operands.first().copied() == Some(argument)
        }),
        "the narrowed owned DOM wrapper read must be released after import"
    );
}

/// Verifies only modern SimpleXML import accepts existing legacy element and attribute kinds.
#[test]
fn modern_simplexml_import_materializes_legacy_kinds_on_every_target() {
    let mut import_module = lower_source(
        r#"<?php
function import_to_modern(SimpleXMLElement $node): mixed {
    return Dom\import_simplexml($node);
}
"#,
    );
    let import_text = print_module(&import_module);
    assert!(
        import_text.contains("internal_extension#4096 flags=2"),
        "modern SimpleXML import lost its locked bridge opcode: {import_text}"
    );

    let mut ordinary_modern_module = lower_source(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
"#,
    );
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        import_module.target = target;
        let import_assembly = generate_user_asm_from_ir(&import_module, false, false)
            .expect("modern SimpleXML import should lower on every supported target");
        ordinary_modern_module.target = target;
        let ordinary_assembly = generate_user_asm_from_ir(&ordinary_modern_module, false, false)
            .expect("ordinary modern wrapper materialization should lower on every target");
        let (legacy_element_compare, legacy_attribute_compare) = match target.arch {
            Arch::AArch64 => ("cmp x0, #101", "cmp x0, #102"),
            Arch::X86_64 => ("cmp rax, 101", "cmp rax, 102"),
        };
        for expected in [legacy_element_compare, legacy_attribute_compare] {
            assert!(
                import_assembly.contains(expected),
                "{target:?} omitted the opcode-4096 legacy-kind exception {expected}"
            );
            assert!(
                !ordinary_assembly.contains(expected),
                "{target:?} broadened ordinary modern wrapper dispatch with {expected}"
            );
        }
    }
}

/// Verifies a terminating false guard narrows a legacy DOM union before wrapper arguments.
#[test]
fn narrows_legacy_dom_wrapper_after_terminating_false_guard() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$element = $document->createElement("root");
if ($element === false) {
    exit(2);
}
$document->appendChild($element);
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4386 flags=3"),
        "missing narrowed DOMNode::appendChild call: {text}"
    );
    assert!(
        text.lines().any(|line| {
            line.contains("internal_extension#4386 flags=3")
                && line.contains("php=DOMNode|false")
        }),
        "appendChild result lost its locked legacy union: {text}"
    );
}

/// Verifies later branch lowering preserves earlier fallthrough-only local narrowing.
#[test]
fn preserves_dom_narrowing_across_consecutive_terminating_guards() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$cdata = $document->createCDATASection("a");
if ($cdata === false) {
    exit(2);
}
$instruction = $document->createProcessingInstruction("pi", "b");
if ($instruction === false) {
    exit(3);
}
echo $cdata->nodeName;
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4626 flags=1"),
        "later guards widened an earlier DOM wrapper back to a generic property read: {text}"
    );
    assert!(
        !text.lines().any(|line| {
            line.contains("prop_get") && line.contains("span: 11:12")
        }),
        "DOMNode::$nodeName leaked to ordinary property storage: {text}"
    );
}

/// Verifies heterogeneous node-list unions defer virtual properties to runtime wrapper dispatch.
#[test]
fn defers_dom_node_list_item_property_to_runtime_wrapper_dispatch() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<root><child/></root>");
$nodes = $document->getElementsByTagName("child");
echo $nodes->item(0)->nodeName;
"#,
    );
    let text = print_module(&module);
    assert!(
        text.lines().any(|line| {
            line.contains("prop_get")
                && line.contains("php=mixed")
                && line.contains("span: 5:21")
        }),
        "DOMNodeList::item() lost runtime nodeName dispatch: {text}"
    );
    assert!(
        !text.contains("internal_extension#4626 flags=1"),
        "DOMNodeList::item() incorrectly selected DOMNode's nodeName opcode: {text}"
    );
}

/// Verifies DOM collection dimensions select typed native methods on every target.
#[test]
fn lowers_dom_collection_dimension_reads_on_every_target() {
    let mut module = lower_source(
        r#"<?php
function legacy(DOMNodeList $nodes, DOMNamedNodeMap $attributes): void {
    $position = 0;
    $name = 'id';
    var_dump($nodes[$position]);
    var_dump($attributes[$position]);
    var_dump($attributes[$name]);
}
function modern(
    Dom\NodeList $nodes,
    Dom\HTMLCollection $elements,
    Dom\NamedNodeMap $attributes,
    Dom\DtdNamedNodeMap $declarations,
): void {
    $position = 0;
    $name = 'id';
    $declaration = 'entity';
    var_dump($nodes[$position]);
    var_dump($elements[$position]);
    var_dump($elements[$name]);
    var_dump($attributes[$position]);
    var_dump($attributes[$name]);
    var_dump($declarations[$position]);
    var_dump($declarations[$declaration]);
}
"#,
    );
    let text = print_module(&module);
    for opcode in [4409, 4381, 4379, 4254, 4212, 4213, 4228, 4226, 4170, 4168] {
        let expected = format!("internal_extension#{opcode} flags=3");
        assert!(
            text.contains(&expected),
            "DOM dimension lowering omitted {expected}: {text}"
        );
    }
    assert!(
        !text.contains("runtime.array_access"),
        "DOM collection dimension leaked to generic runtime access: {text}"
    );

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        generate_user_asm_from_ir(&module, false, false)
            .expect("DOM collection dimension reads should lower on every target");
    }
}

/// Verifies namespace-capable node-list unions preserve wrapper-valued property results.
#[test]
fn preserves_dom_node_list_item_wrapper_property_types() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML("<root/>");
$node = (new DOMXPath($document))->query("//namespace::*")->item(0);
var_dump($node->ownerDocument === $document);
var_dump($node->parentNode === $document->documentElement);
var_dump($node->parentElement === $document->documentElement);
"#,
    );
    let text = print_module(&module);
    for (php_type, property) in [
        ("DOMDocument|null", "ownerDocument"),
        ("DOMNode|null", "parentNode"),
        ("DOMElement|null", "parentElement"),
    ] {
        assert!(
            text.lines()
                .any(|line| line.contains("prop_get") && line.contains(&format!("php={php_type}"))),
            "DOMNodeList::item()->{property} lost wrapper materialization: {text}"
        );
    }
}

/// Verifies runtime-named DOM node properties keep their bridge dispatch on every target.
#[test]
fn lowers_runtime_named_dom_node_properties_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$document = Dom\XMLDocument::createFromString('<root><child/></root>');
$node = $document->documentElement->firstChild;
foreach (['firstChild', 'lastChild', 'parentNode', 'parentElement', 'ownerDocument', 'previousSibling', 'nextSibling', 'textContent', 'childNodes'] as $property) {
    var_dump($node->$property);
}
"#,
    );
    let text = print_module(&module);
    assert!(
        text.lines().any(|line| {
            line.contains("dynamic_prop_get") && line.contains("php=mixed")
        }),
        "runtime-named DOM property access lost its dynamic EIR operation: {text}"
    );

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        generate_user_asm_from_ir(&module, false, false).unwrap_or_else(|error| {
            panic!("{target:?} runtime-named DOM properties failed: {error}")
        });
    }
}

/// Verifies namespace-capable attribute unions retain owner-document wrappers too.
#[test]
fn preserves_dom_attribute_namespace_union_wrapper_property_type() {
    let module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root xmlns:p="urn:p"/>');
$attribute = $document->documentElement->getAttributeNode("xmlns:p");
var_dump($attribute->ownerDocument === $document);
"#,
    );
    let text = print_module(&module);
    assert!(
        text.lines()
            .any(|line| line.contains("prop_get") && line.contains("php=DOMDocument|null")),
        "DOMAttr|DOMNameSpaceNode ownerDocument lost wrapper materialization: {text}"
    );
}

/// Verifies wrapper-only nullable unions route virtual-property writes through the native bridge.
#[test]
fn lowers_nullable_dom_property_write_through_native_bridge() {
    let module = lower_source(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root><child/></root>");
$child = $document->documentElement->firstElementChild;
$child->innerHTML = "<nested/>";
"#,
    );
    let text = print_module(&module);
    assert!(
        text.contains("internal_extension#4660 flags=1"),
        "nullable Dom\\Element::$innerHTML write bypassed the bridge: {text}"
    );
    assert!(
        !text.lines().any(|line| {
            line.contains("prop_set") && line.contains("span: 4:1")
        }),
        "nullable DOM property write leaked to ordinary property storage: {text}"
    );
}

/// Verifies heterogeneous DTD wrapper unions defer virtual-property routing to runtime class dispatch.
#[test]
fn defers_heterogeneous_dtd_property_opcode_selection() {
    let module = lower_source(
        r#"<?php
$document = Dom\XMLDocument::createFromString(
    '<!DOCTYPE r [<!NOTATION n SYSTEM "n.sys">]><r/>'
);
$notation = $document->doctype->notations["n"];
echo $notation->publicId;
"#,
    );
    let text = print_module(&module);
    assert!(
        text.lines().any(|line| {
            line.contains("prop_get") && line.contains("php=mixed")
        }),
        "heterogeneous DTD property union did not retain runtime dispatch: {text}"
    );
    assert!(
        !text.contains("internal_extension#4516 flags=1"),
        "Dom\\Notation::$publicId incorrectly selected Dom\\Entity's opcode: {text}"
    );
}

/// Verifies PHP clone expressions use DOM's deep native object handler.
#[test]
fn lowers_dom_object_clone_through_native_bridge() {
    let module = lower_source(
        r#"<?php
$document = Dom\XMLDocument::createFromString("<root><child/></root>");
$element = $document->documentElement;
if ($element === null) { exit(2); }
$documentCopy = clone $document;
$elementCopy = clone $element;
"#,
    );
    let text = print_module(&module);
    let clone_calls = module
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .filter(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension {
                        opcode: 4109,
                        flags: 3,
                    })
                )
        })
        .count();
    assert_eq!(clone_calls, 2, "native clones bypassed opcode 4109: {text}");
    assert!(
        !module
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .any(|instruction| instruction.op == Op::ObjectCloneShallow),
        "DOM clones leaked to shallow PHP object storage: {text}"
    );
}

/// Verifies direct DOM variadic arguments remain flat bridge operands in source order.
#[test]
fn lowers_dom_variadic_mutation_as_flat_operands() {
    let module = lower_source(
        r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("root");
$a = $document->createElement("a");
$b = $document->createElement("b");
$root->append("prefix", $a, $b);
"#,
    );
    let call = module
        .functions
        .iter()
        .flat_map(|function| function.instructions.iter())
        .find(|instruction| {
            instruction.op == Op::InternalExtensionCall
                && matches!(
                    instruction.immediate,
                    Some(Immediate::InternalExtension { opcode: 4172, .. })
                )
        })
        .expect("missing Dom\\Element::append bridge call");
    assert_eq!(
        call.operands.len(),
        4,
        "receiver and three variadic values must stay flat"
    );
}

/// Verifies x86_64 lowers boxed wrapper unions before runtime `Stringable` coercion.
#[test]
fn lowers_dom_boxed_stringable_preflight_on_linux_x86_64() {
    let mut module = lower_source(
        r#"<?php
class X86DomSelector {
    public function __toString(): string {
        return "root";
    }
}

function boxed_x86_dom_value(mixed $value): mixed {
    return $value;
}

$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement("root");
$child = $document->createElement("child");
boxed_x86_dom_value($root)->append(boxed_x86_dom_value($child));
echo boxed_x86_dom_value($root)->matches(
    boxed_x86_dom_value(new X86DomSelector())
);
"#,
    );
    module.target = Target::new(Platform::Linux, Arch::X86_64);

    let assembly = generate_user_asm_from_ir(&module, false, false)
        .expect("boxed DOM Stringable preflight should lower for Linux x86_64");
    let tostring_symbol = crate::names::method_symbol("X86DomSelector", "__tostring");
    for expected in [
        "call setjmp",
        "call __rt_mixed_instanceof",
        "call __rt_str_persist",
        "jmp __rt_throw_current",
        &format!("call {tostring_symbol}"),
    ] {
        assert!(
            assembly.contains(expected),
            "Linux x86_64 DOM Stringable assembly omitted {expected}: {assembly}"
        );
    }
}

/// Verifies constructorless wrappers retain their finalizer runtime without a bridge call.
#[test]
fn constructorless_dom_wrapper_allocation_requires_dom_runtime() {
    let module = lower_source(
        r#"<?php
$implementation = new DOMImplementation();
"#,
    );

    assert!(module.required_runtime_features.dom_bridge);
}

/// Verifies DOM runtime activation always publishes the native host callback resolvers.
#[test]
fn dom_runtime_without_direct_bridge_call_emits_xpath_resolvers() {
    let module = lower_source(
        r#"<?php
$implementation = new DOMImplementation();
"#,
    );

    let assembly = generate_user_asm_from_ir(&module, false, false)
        .expect("constructorless DOM wrapper module should lower to assembly");
    assert!(
        assembly
            .lines()
            .any(|line| line == "__rt_dom_xpath_resolve_callable:"),
        "DOM runtime assembly omitted its callable-name resolver"
    );
    assert!(
        assembly
            .lines()
            .any(|line| {
                line == "__rt_dom_xpath_resolve_callable_array:"
                    || line == "___rt_dom_xpath_resolve_callable_array:"
            }),
        "DOM runtime assembly omitted its callable-array resolver"
    );
}

/// Verifies a main-only DOM cdylib publishes one resolver pair on every supported target.
#[test]
fn main_only_dom_cdylib_emits_one_xpath_resolver_pair_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$implementation = new DOMImplementation();
"#,
    );
    module
        .functions
        .retain(|function| function.flags.is_main || function.name == "main");
    module.class_methods.clear();
    module.closures.clear();
    assert!(module.required_runtime_features.dom_bridge);
    assert_eq!(module.functions.len(), 1, "fixture must retain only main");

    let exported_functions = HashMap::new();
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir_with_options(
            &module,
            false,
            false,
            false,
            Emit::Cdylib,
            &exported_functions,
            true,
            false,
        )
        .expect("main-only DOM cdylib should lower to assembly");
        let callable_label = "__rt_dom_xpath_resolve_callable:";
        let callable_array_label = format!(
            "{}:",
            target.extern_symbol("__rt_dom_xpath_resolve_callable_array")
        );
        assert_eq!(
            assembly.lines().filter(|line| *line == callable_label).count(),
            1,
            "{target:?} omitted or duplicated the callable-name resolver"
        );
        assert_eq!(
            assembly
                .lines()
                .filter(|line| *line == callable_array_label)
                .count(),
            1,
            "{target:?} omitted or duplicated the callable-array resolver"
        );
    }
}

/// Verifies generic dynamic allocation retains native-wrapper finalization support.
#[test]
fn dynamic_dom_wrapper_allocation_requires_dom_runtime() {
    let module = lower_source(
        r#"<?php
$class = DOMImplementation::class;
$implementation = new $class();
"#,
    );

    assert!(module.required_runtime_features.dom_bridge);
}

/// Verifies direct userland descendant allocation retains native wrapper runtime support.
#[test]
fn native_wrapper_descendant_allocation_requires_dom_runtime() {
    let module = lower_source(
        r#"<?php
class RuntimeFeatureElement extends DOMElement {}
$element = new RuntimeFeatureElement("root");
"#,
    );

    assert!(module.required_runtime_features.dom_bridge);
}

/// Verifies dynamic userland descendant allocation retains native wrapper runtime support.
#[test]
fn dynamic_native_wrapper_descendant_allocation_requires_dom_runtime() {
    let module = lower_source(
        r#"<?php
class DynamicRuntimeFeatureElement extends DOMElement {}
$class = DynamicRuntimeFeatureElement::class;
$element = new $class("root");
"#,
    );

    assert!(module.required_runtime_features.dom_bridge);
}

/// Verifies a Mixed property read that can branch to a virtual DOM property links the bridge.
#[test]
fn mixed_virtual_dom_property_candidate_requires_dom_runtime() {
    let module = lower_source(
        r#"<?php
function opaque_dom_property_candidate(mixed $value): mixed {
    return $value;
}

class DomPropertyCandidateRow {
    public string $name = "Ada";
}

$row = opaque_dom_property_candidate(new DomPropertyCandidateRow());
echo $row->name;
"#,
    );

    assert!(module.required_runtime_features.dom_bridge);
}

/// Verifies a Mixed method call that can branch to a bodyless DOM method links the bridge.
#[test]
fn mixed_bodyless_dom_method_candidate_requires_dom_runtime() {
    let module = lower_source(
        r#"<?php
function opaque_dom_method_candidate(mixed $value): mixed {
    return $value;
}

class DomMethodCandidateSink {
    public function append(string $value): string {
        return $value;
    }
}

$sink = opaque_dom_method_candidate(new DomMethodCandidateSink());
echo $sink->append("ok");
"#,
    );

    assert!(module.required_runtime_features.dom_bridge);
}

/// Verifies an unrelated Mixed property name does not force latent DOM runtime support.
#[test]
fn unrelated_mixed_property_candidate_does_not_require_dom_runtime() {
    let module = lower_source(
        r#"<?php
function opaque_unrelated_property_candidate(mixed $value): mixed {
    return $value;
}

class UnrelatedPropertyCandidateRow {
    public string $elephc_unique_marker = "plain";
}

$row = opaque_unrelated_property_candidate(new UnrelatedPropertyCandidateRow());
echo $row->elephc_unique_marker;
"#,
    );

    assert!(!module.required_runtime_features.dom_bridge);
}

/// Verifies injected DOM declarations alone do not enable the native bridge.
#[test]
fn non_dom_baseline_does_not_require_dom_runtime() {
    let module = lower_source("<?php echo 'plain';");

    assert!(!module.required_runtime_features.dom_bridge);
}

/// Verifies foreach materializes the synthetic DOM collection iterator body in its interface slot.
#[test]
fn lowers_dom_collection_get_iterator_for_foreach_interface_dispatch() {
    let mut module = lower_source(
        r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$xpath = new DOMXPath($document);
foreach ($xpath->query('/root/missing') as $node) {
    var_dump($node);
}
"#,
    );

    assert_eq!(
        module
            .class_infos
            .get("DOMNodeList")
            .and_then(|class_info| class_info.method_impl_classes.get("getiterator"))
            .map(String::as_str),
        Some("DOMNodeList")
    );
    assert!(module.class_methods.iter().any(|function| {
        function.name == "DOMNodeList::getIterator"
    }));
    let class_id = module.class_infos["DOMNodeList"].class_id;
    let interface_id = module.interface_infos["IteratorAggregate"].interface_id;
    let table_label = format!(
        "_class_interface_impl_{class_id}_{interface_id}:"
    );
    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .expect("DOM foreach interface dispatch should lower on every target");
        let table = assembly
            .split_once(&table_label)
            .map(|(_, tail)| tail)
            .expect("DOMNodeList IteratorAggregate table");
        assert!(
            table
                .lines()
                .take(3)
                .any(|line| line.trim() == ".quad _method_DOMNodeList_getiterator"),
            "DOMNodeList getIterator slot remained null on {target}: {table}"
        );
    }
}

/// Verifies the complete SimpleXML/libxml compiler-facing route matrix is
/// retained in EIR and materialized on every supported target.
///
/// The 39 SimpleXML references include both DOM-family imports; the eight
/// libxml references are the complete callable companion surface. Property
/// accessors on `LibXMLError` deliberately remain covered by their independent
/// writable-object regression rather than pretending they are callable routes.
#[test]
fn lowers_all_simplexml_and_libxml_callable_routes_on_every_target() {
    let mut module = lower_source(
        r#"<?php
$xml = new SimpleXMLElement('<root id="before"><item>A</item></root>');
$loadedFile = simplexml_load_file('fixture.xml');
$loadedString = simplexml_load_string('<loaded/>');
$document = new DOMDocument();
$document->loadXML('<dom><child/></dom>');
$element = $document->documentElement;
if ($element === null) { exit(2); }
simplexml_import_dom($element);
dom_import_simplexml($xml);
Dom\import_simplexml($xml);

$xml->__debugInfo();
$xml->__toString();
$xml->addAttribute('added', 'value');
$xml->addChild('added-child', 'value');
$xml->asXML();
$xml->attributes();
$xml->children();
$xml->count();
$xml->current();
$xml->getChildren();
$xml->getDocNamespaces();
$xml->getName();
$xml->getNamespaces();
$xml->hasChildren();
$xml->key();
$xml->next();
$xml->registerXPathNamespace('p', 'urn:p');
$xml->rewind();
$xml->saveXML();
$xml->valid();
$xml->xpath('/root/item');

$array = (array) $xml;
$same = $xml == $xml;
$count = count($xml);
foreach ($xml as $item) { break; }
isset($xml['id']);
isset($xml->item);
$attribute = $xml['id'];
$child = $xml->item;
unset($xml['id']);
unset($xml->item);
$xml['id'] = 'after';
$xml->item = 'B';

libxml_clear_errors();
libxml_disable_entity_loader();
libxml_get_errors();
libxml_get_external_entity_loader();
libxml_get_last_error();
libxml_set_external_entity_loader(null);
libxml_set_streams_context(stream_context_create(['http' => ['method' => 'GET']]));
libxml_use_internal_errors(true);
"#,
    );
    let routes: &[(&str, u32)] = &[
        ("simplexml.import.legacy-source", 4106),
        ("simplexml.import.legacy-target", 4097),
        ("simplexml.import.modern-target", 4096),
        ("simplexml.load-file", 4107),
        ("simplexml.load-string", 4108),
        ("simplexml.construct", 4425),
        ("simplexml.debug-info", 4426),
        ("simplexml.to-string", 4427),
        ("simplexml.add-attribute", 4428),
        ("simplexml.add-child", 4429),
        ("simplexml.as-xml", 4430),
        ("simplexml.attributes", 4431),
        ("simplexml.children", 4432),
        ("simplexml.method-count", 4433),
        ("simplexml.current", 4434),
        ("simplexml.get-children", 4435),
        ("simplexml.get-doc-namespaces", 4436),
        ("simplexml.get-name", 4437),
        ("simplexml.get-namespaces", 4438),
        ("simplexml.has-children", 4439),
        ("simplexml.key", 4440),
        ("simplexml.next", 4441),
        ("simplexml.register-xpath-namespace", 4442),
        ("simplexml.rewind", 4443),
        ("simplexml.save-xml", 4444),
        ("simplexml.valid", 4445),
        ("simplexml.xpath", 4446),
        ("simplexml.handler-cast", 4447),
        ("simplexml.handler-compare", 4448),
        ("simplexml.handler-count", 4449),
        ("simplexml.handler-get-iterator", 4450),
        ("simplexml.handler-has-dimension", 4451),
        ("simplexml.handler-has-property", 4452),
        ("simplexml.handler-read-dimension", 4453),
        ("simplexml.handler-read-property", 4454),
        ("simplexml.handler-unset-dimension", 4455),
        ("simplexml.handler-unset-property", 4456),
        ("simplexml.handler-write-dimension", 4457),
        ("simplexml.handler-write-property", 4458),
        ("libxml.clear-errors", 4098),
        ("libxml.disable-entity-loader", 4099),
        ("libxml.get-errors", 4100),
        ("libxml.get-external-entity-loader", 4101),
        ("libxml.get-last-error", 4102),
        ("libxml.set-external-entity-loader", 4103),
        ("libxml.set-streams-context", 4104),
        ("libxml.use-internal-errors", 4105),
    ];
    assert_eq!(routes.len(), 47, "locked callable route reference count");

    let text = print_module(&module);
    for (case_id, opcode) in routes {
        assert!(
            text.contains(&format!("internal_extension#{opcode}")),
            "{case_id} omitted opcode {opcode} from EIR: {text}"
        );
    }

    for target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        module.target = target;
        let assembly = generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target:?} route matrix failed: {error}"));
        for (case_id, opcode) in routes {
            assert!(
                assembly.contains(&opcode.to_string()),
                "{target:?} {case_id} omitted opcode {opcode}: {assembly}"
            );
        }
    }
}
