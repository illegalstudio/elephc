//! Purpose:
//! Emits EIR backend dispatchers for include-loaded function variants.
//! Interprets variant metadata lowered from resolver-produced synthetic statements.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit` before user functions are emitted.
//! - `crate::codegen_ir::lower_inst` when a concrete include path activates a variant.
//!
//! Key details:
//! - Dispatchers use the public PHP function symbol and tail-dispatch through an
//!   active function-pointer slot populated by `FunctionVariantMark`.

use crate::codegen::abi;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::{function_variants, Function, Module};
use crate::names::{function_symbol, function_variant_active_symbol};

// Delegate pure variant resolution/collect to the canonical ir module (single source of truth).
pub(super) use function_variants::{
    collect_dispatch_groups, parse_variant_label, FunctionVariantLabel,
};

/// Returns a representative concrete variant function for a public function group.
pub(super) fn variant_callee_for_group<'a>(module: &'a Module, name: &str) -> Option<&'a Function> {
    function_variants::variant_callee_for_group(module, name)
}

/// Emits every include-variant dispatcher required by the EIR module.
pub(super) fn emit_dispatchers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    for group in collect_dispatch_groups(module) {
        emit_function_variant_dispatcher(emitter, data, &group.name);
    }
}

/// Emits the runtime mark that makes one concrete include-loaded function active.
pub(super) fn emit_variant_mark(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &FunctionVariantLabel,
) -> crate::codegen_ir::Result<()> {
    if label.variants.len() != 1 {
        return Err(crate::codegen_ir::CodegenIrError::invalid_module(format!(
            "function variant mark for '{}' names {} variants",
            label.name,
            label.variants.len()
        )));
    }
    let variant = &label.variants[0];
    let active_symbol = function_variant_active_symbol(&label.name);
    data.add_comm(active_symbol.clone(), 8);

    let variant_reg = abi::temp_int_reg(emitter.target);
    abi::emit_symbol_address(emitter, variant_reg, &function_symbol(variant));
    abi::emit_store_reg_to_symbol(emitter, variant_reg, &active_symbol, 0);
    Ok(())
}

/// Emits a public-name thunk that tail-dispatches to the currently active variant.
fn emit_function_variant_dispatcher(
    emitter: &mut Emitter,
    data: &mut DataSection,
    name: &str,
) {
    let label = function_symbol(name);
    let active_symbol = function_variant_active_symbol(name);
    data.add_comm(active_symbol.clone(), 8);

    let fail_label = format!("{}_undefined_variant", label);
    let message = format!("Fatal error: Call to undefined function {}()\n", name);
    let (message_label, message_len) = data.add_string(message.as_bytes());
    let target_reg = abi::symbol_scratch_reg(emitter);

    emitter.raw(".align 2");
    emitter.label_global(&label);
    abi::emit_load_symbol_to_reg(emitter, target_reg, &active_symbol, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", target_reg, fail_label)); // abort if no include has activated this function variant
            emitter.instruction(&format!("br {}", target_reg));                 // tail-dispatch to the active function variant with existing arguments
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", target_reg, target_reg)); // abort if no include has activated this function variant
            emitter.instruction(&format!("je {}", fail_label));                 // jump to the undefined-function fatal path
            emitter.instruction(&format!("jmp {}", target_reg));                // tail-dispatch to the active function variant with existing arguments
        }
    }

    emitter.label(&fail_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the undefined-function diagnostic to stderr
            emitter.adrp("x1", &message_label);                                 // load the diagnostic string page for stderr output
            emitter.add_lo12("x1", "x1", &message_label);                      // resolve the diagnostic string address for stderr output
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the undefined-function diagnostic to Linux stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal diagnostic before terminating
            abi::emit_exit(emitter, 1);
        }
    }
}

// (pure helpers moved to crate::ir::function_variants for canonical single implementation)
