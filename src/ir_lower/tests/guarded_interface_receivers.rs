//! Purpose:
//! Verify guarded interface receiver metadata across every supported native target.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Checks the receiver value separately from its unchanged interface-typed local slot.

use crate::codegen::platform::Target;
use crate::ir::Op;
use crate::types::PhpType;
use std::path::Path;

/// Checked concrete and child-interface views survive lowering and optimized target emission.
#[test]
fn guarded_interface_receiver_views_preserve_storage_on_all_targets() {
    let source = r#"<?php
interface Endpoint {}
interface ActiveEndpoint extends Endpoint { public function value(): int; }
final class Reference implements ActiveEndpoint {
    public int $type = 7;
    public ?string $label = "ok";
    public int $pending;
    public float $weight;
    public ?string $note;
    public function value(): int { return $this->type; }
}
final class Direct implements Endpoint {}
final class Buffered implements Endpoint { public buffer<int> $data; }
function property(Endpoint $endpoint): int {
    if (!($endpoint instanceof Reference)) { return 0; }
    return $endpoint->type;
}
function method(Endpoint $endpoint): int {
    if ($endpoint instanceof ActiveEndpoint) { return $endpoint->value(); }
    return 0;
}
function label(Endpoint $endpoint): string {
    if (!($endpoint instanceof Reference)) { return "other"; }
    if ($endpoint->label === null) { return "null"; }
    return $endpoint->label;
}
function probe(Endpoint $endpoint): void {
    if ($endpoint instanceof Reference) {
        echo isset($endpoint->pending), empty($endpoint->pending);
        echo $endpoint->pending ?? "plain", $endpoint?->pending ?? "safe";
    }
}
function loop_probe(Endpoint $endpoint): void {
    for ($i = 0; $i < 2; $i++) {
        if ($endpoint instanceof Reference) {
            $label = $endpoint->label;
            $set = isset($endpoint->pending);
            $safe = $endpoint?->pending ?? "safe";
            echo $label, $set, $safe;
        }
        if ($endpoint instanceof ActiveEndpoint) {
            $value = $endpoint->value();
            echo $value;
        }
    }
}
function closure_probe(Endpoint $endpoint): void {
    for ($i = 0; $i < 1; $i++) {
        if ($endpoint instanceof Reference) {
            $read = function () use ($endpoint): int { return $endpoint->type; };
            echo $read();
        }
        if ($endpoint instanceof ActiveEndpoint) {
            $call = fn(): int => $endpoint->value();
            echo $call();
        }
        $named = guarded_value($endpoint);
        echo $named;
    }
}
function guarded_value(Endpoint $endpoint): int {
    if ($endpoint instanceof Reference) { return $endpoint->type; }
    return 0;
}
function main(Endpoint $endpoint): int {
    if ($endpoint instanceof Reference) { return $endpoint->type; }
    return 0;
}
function dynamic(Endpoint $endpoint, string $name, string $pending): void {
    if ($endpoint instanceof Reference) {
        echo $endpoint->{'type'}, $endpoint?->{'type'}, $endpoint->$name, $endpoint?->{$name};
        echo $endpoint->{'pending'} ?? "plain", $endpoint?->{$pending} ?? "safe";
        echo $endpoint->{'weight'} ?? "float", $endpoint?->{'note'} ?? "string";
    }
}
function buffer_probe(Endpoint $endpoint, string $name): void {
    if ($endpoint instanceof Buffered) {
        $value = $endpoint->$name ?? throw new RuntimeException("missing");
        echo $value[0];
    }
}
function choose(bool $active): Endpoint {
    if ($active) { return new Reference(); }
    return new Direct();
}
$endpoint = choose($argc > 1);
echo property($endpoint), method($endpoint), label($endpoint);
probe($endpoint);
loop_probe($endpoint);
closure_probe($endpoint);
dynamic($endpoint, "type", "pending");
for ($global = 0; $global < 1; $global++) {
    $value = main($endpoint);
    echo $value;
}
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = super::lower_source_at_for_target(
            source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
        );
        for (name, op, expected) in [
            ("property", Op::PropGet, "Reference"),
            ("method", Op::MethodCall, "ActiveEndpoint"),
            ("label", Op::PropGet, "Reference"),
            ("probe", Op::PropInitialized, "Reference"),
            ("loop_probe", Op::PropGet, "Reference"),
            ("loop_probe", Op::PropInitialized, "Reference"),
            ("loop_probe", Op::MethodCall, "ActiveEndpoint"),
            ("guarded_value", Op::PropGet, "Reference"),
            ("main", Op::PropGet, "Reference"),
            ("dynamic", Op::DynamicPropGet, "Reference"),
            ("buffer_probe", Op::DynamicPropGet, "Buffered"),
        ] {
            let function = module.functions.iter().find(|function| function.name == name).unwrap();
            let member = function.instructions.iter().find(|inst| inst.op == op).unwrap();
            if name == "buffer_probe" {
                assert_eq!(member.result_php_type, PhpType::Buffer(Box::new(PhpType::Int)));
            }
            let receiver = function.value(member.operands[0]).unwrap();
            assert_eq!(receiver.php_type, PhpType::Object(expected.into()), "{target}: {name}");
            let mut load = function.instructions.iter().find(|inst| inst.result == Some(member.operands[0])).unwrap();
            // Calls on current main acquire a separate owner after the borrowed receiver view.
            if load.op == Op::Acquire {
                assert_eq!(op, Op::MethodCall, "{target}: {name}");
                assert_eq!(receiver.ownership, crate::ir::Ownership::Owned);
                load = function.instructions.iter().find(|inst| inst.result == Some(load.operands[0])).unwrap();
            }
            assert_eq!(load.op, Op::LoadLocal, "{target}: {name}: refinement preserves the producer");
            let view = function.value(load.result.unwrap()).unwrap();
            assert_eq!(view.php_type, receiver.php_type);
            assert_eq!(load.result_php_type, view.php_type);
            assert_eq!(load.result_ownership, view.ownership);
            let slot = function.locals.iter().find(|slot| slot.name.as_deref() == Some("endpoint")).unwrap();
            assert_eq!(slot.php_type, PhpType::Object("Endpoint".into()), "{target}: {name}");
        }
        for optimized in [false, true] {
            if optimized { crate::ir_passes::optimize_module(&mut module); }
            crate::ir::validate_module(&module).expect("valid refined receiver EIR");
            crate::codegen::generate_user_asm_from_ir(&module, false, false)
                .unwrap_or_else(|error| panic!("{target}, optimized={optimized}: {error:?}"));
        }
    }
}
