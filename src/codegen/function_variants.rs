//! Purpose:
//! Emits include-aware function variant thunks and active-symbol checks for resolved includes.
//! Keeps multiple discovered function bodies callable through a stable PHP function name.
//!
//! Called from:
//! - `crate::codegen::generate()` after resolver-provided variant metadata
//!
//! Key details:
//! - Variant symbols are coupled to include statements and must preserve PHP load-order behavior.

use std::collections::HashMap;

use crate::codegen::platform::Arch;
use crate::names::{function_symbol, function_variant_active_symbol};
use crate::parser::ast::{Program, Stmt, StmtKind};

use super::abi;
use super::data_section::DataSection;
use super::emit::Emitter;

pub(crate) fn collect_function_variant_groups(program: &Program) -> HashMap<String, Vec<String>> {
    let mut groups = HashMap::new();
    collect_from_stmts(program, &mut groups);
    groups
}

pub(crate) fn emit_function_variant_dispatcher(
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
            emitter.instruction(&format!("cbz {}, {}", target_reg, fail_label)); // abort if no include has loaded this function implementation
            emitter.instruction(&format!("br {}", target_reg));                 // tail-dispatch to the loaded function variant without changing arguments
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", target_reg, target_reg)); // abort if no include has loaded this function implementation
            emitter.instruction(&format!("je {}", fail_label));                 // jump to the fatal path when the active function pointer is missing
            emitter.instruction(&format!("jmp {}", target_reg));                // tail-dispatch to the loaded function variant without changing arguments
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

fn collect_from_stmts(stmts: &[Stmt], groups: &mut HashMap<String, Vec<String>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::FunctionVariantGroup { name, variants } => {
                groups.insert(name.clone(), variants.clone());
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. } => {
                collect_from_stmts(body, groups);
            }
            StmtKind::IncludeOnceGuard { body, .. } => {
                collect_from_stmts(body, groups);
            }
            _ => {}
        }
    }
}
