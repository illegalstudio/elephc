//! Purpose:
//! Owns managed canonical cells behind native static-property bindings.
//!
//! Called from:
//! - Explicit native-validation ABI fixtures and the static-storage integration.
//!
//! Key details:
//! - A binding reserves value, initialization and promoted-layout words, in that order.
//! - Only the allocated cell has a managed heap header; the static binding never does.
//! - Promotion transfers the existing Mixed owner, preserving any escaped PHP wrapper.
//! - Retirement detaches the binding and its weak source before PHP cleanup can reenter.

use crate::codegen_support::{abi, emit::Emitter};
pub(crate) use elephc_memoir::reference_cells::static_cells::BINDING_BYTES;
use elephc_memoir::reference_cells::static_cells::{emit_payload, emit_promote};

/// Loads the borrowed PHP value owned by a raw or promoted static Mixed binding.
/// The promoted cell owns its wrapper; readers expose the current child without retaining it.
pub(crate) fn load_mixed_value(emitter: &mut Emitter, symbol: &str, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    if native_validation {
        abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), symbol);
        abi::emit_call_label(emitter, "__rt_reference_static_mixed_payload");
        abi::emit_load_from_address(emitter, result, result, 0);
    } else {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
    }
    abi::emit_call_label(emitter, "__rt_mixed_deref");
}

/// Emits the optional static-binding ABI without changing ordinary symbol-backed storage.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_payload(emitter);
    emit_promote(emitter);
    emit_release(emitter);
}

/// Retires static storage after the compiler has detached its weak type-source metadata.
fn emit_release(emitter: &mut Emitter) {
    elephc_memoir::reference_cells::static_cells::emit_release(emitter, unbind_source);
}

/// Preserves native source-index mutation and failure policy in the compiler.
fn unbind_source(emitter: &mut Emitter) {
    let unbind = emitter.target.extern_symbol("__elephc_reference_sources_unbind_v1");
    abi::emit_call_label(emitter, &unbind);
    super::source_lifecycle::require_success(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported ABI allocates a real cell and detaches its binding before PHP retirement.
    #[test]
    fn static_mixed_reference_cells_transfer_and_retire_on_every_target() {
        assert_eq!(BINDING_BYTES, 24);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(crate::codegen_support::platform::Target::parse(name).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let (promote, release) = assembly.split_once("__rt_reference_static_mixed_release:").unwrap();
            assert!(promote.find("__rt_reference_cell_new").unwrap()
                < promote.find("__rt_reference_cell_promote_mixed").unwrap(), "{name}: allocate before canonical promotion");
            assert!(!promote.contains("__rt_reference_cell_owner"), "{name}: a BSS binding has no heap header");
            let detached = release.find(if name == "linux-x86_64" {
                "mov QWORD PTR [r10 + 16], 0"
            } else { "str xzr, [x10, #16]" }).unwrap();
            let unbind = release.find("__elephc_reference_sources_unbind_v1").unwrap();
            let retire = release.find("__rt_reference_cell_release").unwrap();
            assert!(detached < unbind && unbind < retire, "{name}: detachment and weak metadata precede PHP cleanup");
        }
    }

    /// Native and Reflection reads share a borrowed payload, while main cleanup retires the binding.
    #[test]
    fn static_mixed_reference_readers_and_main_cleanup_cover_every_target() {
        let source = r#"<?php
class StaticCellReadBase { public static mixed $value = 41; }
class StaticCellReadChild extends StaticCellReadBase {}
function staticCellReadViews(): void {
    echo StaticCellReadBase::$value, '|', StaticCellReadChild::$value, '|';
    $class = new ReflectionClass(StaticCellReadChild::class);
    echo $class->getStaticPropertyValue('value'), '|';
    $values = $class->getStaticProperties(); echo $values['value'], '|';
}
staticCellReadViews();
"#;
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut module = crate::ir_lower::tests::lower_source_at_for_target(
                source, std::path::Path::new("main.php"), std::path::Path::new("."), target,
            );
            crate::ir_lower::enable_native_reference_validation(&mut module).unwrap();
            crate::ir::validate_module(&module).unwrap();
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(assembly.matches("__rt_reference_static_mixed_payload").count() >= 4,
                "{name}: native, inherited and both Reflection views resolve the binding");
            assert!(assembly.contains("__rt_reference_static_mixed_release"),
                "{name}: main cleanup must release the managed cell rather than its child box");
            assert!(assembly.contains("__rt_cleanup_preserve_exception"),
                "{name}: a throwing child cannot skip later static owners");
        }
    }

}
