//! Purpose:
//! Defines the table-driven compiler contract for PHP 8.5 legacy and modern DOM opcode lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - The generated operation manifest supplies exhaustive public-member coverage without copying bodies.
//! - Representative source rows pin call planning, result unions, virtual access, ownership, and all targets.

use crate::codegen::generate_user_asm_from_ir;
use crate::codegen::platform::{Arch, Platform, Target};
use crate::internal_extensions::{operation_registry, registry};
use crate::ir::{print_module, Op};

use super::lower_source;

/// One representative PHP source route which must resolve through generated DOM metadata.
struct RouteCase {
    id: &'static str,
    source: &'static str,
    operation_keys: &'static [(&'static str, &'static str, &'static str)],
    result_markers: &'static [&'static str],
}

/// Resolves a manifest method key and reports the stable opcode it owns.
fn method_opcode(class: &str, method: &str) -> u32 {
    operation_registry()
        .method(class, method)
        .unwrap_or_else(|| panic!("missing DOM operation manifest row for {class}::{method}"))
        .opcode
}

/// Resolves a manifest virtual property key and reports the stable opcode it owns.
fn property_opcode(class: &str, property: &str, write: bool) -> u32 {
    operation_registry()
        .property(class, property, write)
        .unwrap_or_else(|| {
            let access = if write { "write" } else { "read" };
            panic!("missing DOM operation manifest row for {access} {class}::${property}")
        })
        .opcode
}

/// Verifies every public, dispatchable DOM member has a generated native operation manifest row.
///
/// This is the exhaustive half of the compiler test plan: individual source fixtures below select
/// route families rather than reimplementing the 313-member DOM surface by hand. `DOMException`
/// is intentionally excluded because it is a compiler-resident PHP exception value, not a bridge
/// wrapper. Engine-only/private constructors are likewise non-dispatchable from valid PHP source.
#[test]
fn dom_manifest_covers_every_public_dispatchable_method_and_property() {
    let surface = registry();
    let operations = operation_registry();
    let mut missing = Vec::new();

    for class in surface.classes().filter(|class| {
        class.extension == "dom"
            && !class.interface
            && !class.enum_type
            && class.canonical_name != "DOMException"
    }) {
        for method in class
            .methods
            .iter()
            .filter(|method| method.public && !method.abstract_method)
        {
            if operations
                .method(&class.canonical_name, &method.signature.name)
                .is_none()
            {
                missing.push(format!("method:{}::{}", class.canonical_name, method.signature.name));
            }
        }
        for property in class.properties.iter().filter(|property| property.public) {
            if operations
                .property(&class.canonical_name, &property.name, false)
                .is_none()
            {
                missing.push(format!("property-get:{}::${}", class.canonical_name, property.name));
            }
            if property.writable
                && operations
                    .property(&class.canonical_name, &property.name, true)
                    .is_none()
            {
                missing.push(format!("property-set:{}::${}", class.canonical_name, property.name));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "DOM operation manifest lacks dispatchable members: {}",
        missing.join(", ")
    );
}

/// Verifies representative route families retain their bridge call, result, ownership, and ABI contracts.
#[test]
fn dom_opcode_route_family_matrix_lowers_and_generates_for_every_supported_target() {
    let cases = [
        RouteCase {
            id: "DOM-EIR-CONSTRUCTION-01",
            source: r#"<?php
$document = new DOMDocument('1.0', 'UTF-8');
$document->loadXML('<root/>');
echo $document->saveXML();
"#,
            operation_keys: &[
                ("method", "DOMDocument", "__construct"),
                ("method", "DOMDocument", "loadXML"),
                ("method", "DOMDocument", "saveXML"),
            ],
            result_markers: &["php=DOMDocument", "php=string|false"],
        },
        RouteCase {
            id: "DOM-EIR-VIRTUAL-PROPERTY-02",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root>before</root>');
$root = $document->documentElement;
if ($root !== null) {
    echo $root->nodeName;
    $root->textContent = 'after';
}
"#,
            operation_keys: &[
                ("property-get", "DOMDocument", "documentElement"),
                ("property-get", "DOMNode", "nodeName"),
                ("property-set", "DOMNode", "textContent"),
            ],
            result_markers: &["php=DOMElement|null", "php=string"],
        },
        RouteCase {
            id: "DOM-EIR-NAMED-SPREAD-03",
            source: r#"<?php
$document = new DOMDocument();
$element = $document->createElement(...['localName' => 'root']);
if ($element !== false) {
    $document->appendChild($element);
}
$document->loadXML(source: '<root/>');
"#,
            operation_keys: &[
                ("method", "DOMDocument", "createElement"),
                ("method", "DOMNode", "appendChild"),
                ("method", "DOMDocument", "loadXML"),
            ],
            result_markers: &["php=DOMElement|false", "php=DOMNode|false"],
        },
        RouteCase {
            id: "DOM-EIR-CALLABLE-04",
            source: r#"<?php
function invoke(callable $callback, string $xml): bool {
    return $callback($xml);
}
$document = new DOMDocument();
$loader = $document->loadXML(...);
var_dump(invoke($loader, '<root/>'));
"#,
            operation_keys: &[("method", "DOMDocument", "loadXML")],
            result_markers: &["php=bool"],
        },
        RouteCase {
            id: "DOM-EIR-VARIADIC-COLLECTION-05",
            source: r#"<?php
$document = Dom\XMLDocument::createEmpty();
$root = $document->createElement('root');
$document->appendChild($root);
$root->replaceChildren('first', 'second');
$nodes = $document->getElementsByTagName('*');
var_dump($nodes->item(0));
"#,
            operation_keys: &[
                ("method", "Dom\\XMLDocument", "createEmpty"),
                ("method", "Dom\\Document", "createElement"),
                ("method", "Dom\\Node", "appendChild"),
                ("method", "Dom\\Element", "replaceChildren"),
                ("method", "Dom\\Document", "getElementsByTagName"),
                ("method", "Dom\\NodeList", "item"),
            ],
            result_markers: &["php=Dom\\NodeList", "php=Dom\\Node|null"],
        },
        RouteCase {
            id: "DOM-EIR-RELEASE-06",
            source: r#"<?php
$document = new DOMDocument();
$document->loadXML('<root/>');
$node = $document->documentElement;
if ($node !== null) {
    echo $document->saveXML($node);
}
"#,
            operation_keys: &[("method", "DOMDocument", "saveXML")],
            result_markers: &["php=string|false"],
        },
    ];

    let targets = [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ];

    for case in cases {
        let mut module = lower_source(case.source);
        let text = print_module(&module);
        assert!(
            module.required_runtime_features.dom_bridge,
            "{}: DOM source did not auto-link the native bridge: {text}",
            case.id
        );

        for (kind, class, member) in case.operation_keys {
            let opcode = match *kind {
                "method" => method_opcode(class, member),
                "property-get" => property_opcode(class, member, false),
                "property-set" => property_opcode(class, member, true),
                unsupported => panic!("{}: unsupported route kind {unsupported}", case.id),
            };
            assert!(
                text.contains(&format!("internal_extension#{opcode}")),
                "{}: missing {kind} {class}::{member} (opcode {opcode}): {text}",
                case.id,
            );
        }
        for marker in case.result_markers {
            assert!(
                text.contains(marker),
                "{}: result signature marker {marker:?} missing: {text}",
                case.id,
            );
        }

        if case.id == "DOM-EIR-RELEASE-06" {
            let main = module
                .functions
                .iter()
                .find(|function| function.name == "main")
                .expect("release route is missing main EIR");
            assert!(
                main.instructions.iter().any(|instruction| instruction.op == Op::Release),
                "{}: DOM wrapper argument/result ownership omitted release: {text}",
                case.id,
            );
        }

        for target in targets {
            module.target = target;
            let assembly = generate_user_asm_from_ir(&module, false, false)
                .unwrap_or_else(|error| panic!("{}: {target:?} emission failed: {error}", case.id));
            assert!(
                assembly.contains("elephc_dom_call"),
                "{}: {target:?} omitted DOM bridge ABI call",
                case.id,
            );
        }
    }
}
