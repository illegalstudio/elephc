//! Purpose:
//! Regression coverage for conditional readonly property writes on all supported targets.
//!
//! Called from:
//! - The AST-to-EIR unit suite through `crate::ir_lower::tests`.
//!
//! Key details:
//! - Readonly violations belong to the fallback branch, not the initialized-value branch.
//! - Emission must also support explicit and implicit setter reflection metadata.

use crate::codegen::platform::Target;
use crate::ir::{Op, Terminator};
use std::path::Path;

/// Verifies the readonly fallback graph and target-aware reflection emission.
fn check_conditional_readonly_write(target: &str) {
    let source = r#"<?php
class ConditionalReadonly {
    public(set) readonly int $explicit;
    public readonly int $implicit;
    public readonly ?int $nullable;
}
function fallback(): int { echo "fallback"; return 9; }
function update(ConditionalReadonly $box): void { $box->implicit ??= fallback(); }
function updateNullable(ConditionalReadonly $box): void { $box->nullable ??= fallback(); }
function overwrite(ConditionalReadonly $box): void { $box->implicit = fallback(); }
$box = new ConditionalReadonly();
try { update($box); } catch (Error $e) { echo "error"; }
$properties = (new ReflectionClass(ConditionalReadonly::class))->getProperties(ReflectionProperty::IS_PROTECTED_SET);
echo count($properties);
"#;
    let module = super::lower_source_at_for_target(
        source, Path::new("main.php"), Path::new("."), Target::parse(target).unwrap(),
    );
    for name in ["update", "updateNullable"] {
        let function = module.functions.iter().find(|function| function.name == name).unwrap();
        let insert = function.blocks.iter().find(|block| block.name == "coalesce_assign.default").unwrap();
        assert!(matches!(insert.terminator, Some(Terminator::Throw { .. })), "{target}: fallback throws");
        let merge = function.blocks.iter().find(|block| block.name == "coalesce_assign.merge").unwrap();
        assert!(!matches!(merge.terminator, Some(Terminator::Throw { .. })), "{target}: keep does not throw");
        let probe = function.blocks.iter().find(|block| block.name == "coalesce.property.merge").unwrap();
        assert!(probe.instructions.iter().any(|id| function.instruction(*id).unwrap().op == Op::UnsetLocal),
            "{target}: the consumed probe cannot remain in its owner slot");
    }
    let overwrite = module.functions.iter().find(|function| function.name == "overwrite").unwrap();
    assert!(overwrite.blocks.iter().any(|block| matches!(block.terminator,
        Some(Terminator::Throw { .. }))), "{target}: direct readonly writes remain catchable");
    crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
}

/// Conditional readonly writes and reflection metadata emit on macOS ARM64.
#[test]
fn conditional_readonly_write_macos() { check_conditional_readonly_write("macos-aarch64"); }

/// Conditional readonly writes and reflection metadata emit on iOS devices.
#[test]
fn conditional_readonly_write_ios_device() { check_conditional_readonly_write("ios-arm64"); }

/// Conditional readonly writes and reflection metadata emit on the iOS Simulator.
#[test]
fn conditional_readonly_write_ios_simulator() { check_conditional_readonly_write("ios-sim-arm64"); }

/// Conditional readonly writes and reflection metadata emit on Linux ARM64.
#[test]
fn conditional_readonly_write_linux_arm64() { check_conditional_readonly_write("linux-aarch64"); }

/// Conditional readonly writes and reflection metadata emit on Linux x86_64.
#[test]
fn conditional_readonly_write_linux_x86_64() { check_conditional_readonly_write("linux-x86_64"); }
