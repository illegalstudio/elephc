//! Purpose:
//! Verifies actual native child probes and their temporary-only cost on every target.
//!
//! Called from:
//! - AST-to-EIR and target emitter CI.
//!
//! Key details:
//! - Raw dynamic reads bypass getters and return independently owned PHP values.
//! - Borrowed ordinary receivers preserve their existing direct property-array path.

use crate::ir::{Immediate, Op, RuntimeCallTarget, RuntimeFnId};

/// Actual XML child selection supports concrete and erased temporaries on every supported ABI.
#[test]
fn actual_native_property_child_probes_on_every_target() {
    let source = r#"<?php
interface TargetGenericProbeMarker {}
class TargetGenericProbe implements TargetGenericProbeMarker {
    public mixed $items;
    public function __construct() { $this->items = (new SimpleXMLElement('<r><item>1</item></r>'))->item; }
    public function targetGenericProbeThisHot(): void { $this->items[] = 2; }
}
#[AllowDynamicProperties]
class TargetGenericProbeDynamic {
    public function __get(string $name): mixed { return null; }
}
class TargetGenericProbeHook {
    public ?SimpleXMLElement $items { get { return $this->items; } }
    public function __construct() { $this->items = (new SimpleXMLElement('<r><item>1</item></r>'))->item; }
}
function targetGenericProbeSeed(mixed $box): void { $box->items = (new SimpleXMLElement('<r><item>1</item></r>'))->item; }
function targetGenericProbeMake(): mixed { return new TargetGenericProbe(); }
function targetGenericProbeKnown(): TargetGenericProbe { return new TargetGenericProbe(); }
function targetGenericProbeObject(): object { return new TargetGenericProbe(); }
function targetGenericProbeInterface(): TargetGenericProbeMarker { return new TargetGenericProbe(); }
function targetGenericProbeHash(): mixed { $box = new TargetGenericProbeDynamic(); targetGenericProbeSeed($box); return $box; }
function targetGenericProbeHook(): mixed { return new TargetGenericProbeHook(); }
function targetGenericProbeComputed(): void {
    targetGenericProbeMake()->items[] = 2;
    targetGenericProbeKnown()->items[0] = 3;
    targetGenericProbeObject()->items[0] .= 'x';
    targetGenericProbeInterface()->items[] = 2;
    targetGenericProbeHash()->items[] = 2;
    targetGenericProbeHook()->items[0] = 3;
}
function targetGenericProbeLate(int $key): void { targetGenericProbeMake()->items[$key] = 'x'; }
function targetGenericProbeBorrowed(TargetGenericProbe $box): void { $box->items[] = 2; }
targetGenericProbeComputed(); targetGenericProbeLate(0);
$box = new TargetGenericProbe(); targetGenericProbeBorrowed($box); $box->targetGenericProbeThisHot();
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(source, std::path::Path::new("main.php"),
            std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
        let computed = module.functions.iter().find(|function| function.name == "targetGenericProbeComputed").unwrap();
        assert!(computed.instructions.iter().any(|inst| inst.immediate == Some(Immediate::RuntimeCall(
            RuntimeCallTarget::PropertyHashRead))), "{target}: stored getter-class entries need a raw read");
        assert!(computed.instructions.iter().any(|inst| inst.op == Op::PropInitialized),
            "{target}: an ordinary probe must skip uninitialized storage without reading it");
        let borrowed = module.functions.iter().filter(|function| function.name == "targetGenericProbeBorrowed"
            || (function.flags.is_method && function.name.to_ascii_lowercase().contains("targetgenericprobethishot")))
            .collect::<Vec<_>>();
        assert_eq!(borrowed.len(), 2, "{target}: both borrowed receiver fixtures must remain reachable");
        for function in borrowed {
            assert!(function.instructions.iter().any(|inst| inst.op == Op::PropGetForWrite),
                "{target}: {} must keep the direct attached writer", function.name);
            assert!(!function.instructions.iter().any(|inst| matches!(inst.immediate,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::PropertyHashRead))
                    | Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(RuntimeFnId::GetClass))))),
                "{target}: {} must not acquire a temporary-child class probe", function.name);
        }
        crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    }
}
