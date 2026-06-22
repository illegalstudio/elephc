//! Purpose:
//! Lowers late static binding and visibility branch emission.
//! Works with static property symbols and class metadata instead of local frame slots.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::static_properties`
//!
//! Key details:
//! - Late-bound receivers and visibility checks must match PHP inheritance semantics before storage is updated.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::static_property_symbol;
use crate::parser::ast::{StaticReceiver, Visibility};
use crate::types::PhpType;

const STATIC_PROP_PRIVATE_ACCESS_LABEL: &str = "_static_prop_private_access_msg";
const STATIC_PROP_PRIVATE_ACCESS_MSG: &str =
    "Fatal error: Cannot access private static property\n";

/// Describes a single late-bound dispatch branch for a static property.
/// Each branch represents a redeclared static property in a descendant class with its class ID and visibility.
#[derive(Clone)]
pub(super) struct StaticPropertyBranch {
    pub(super) class_id: u64,
    pub(super) declaring_class: String,
    pub(super) private_inaccessible: bool,
}

/// Loads the called class ID (from `__elephc_called_class_id` or `$this`) and pushes it onto the stack for late-bound dispatch.
/// Returns `true` if branches exist and the class ID was pushed; `false` otherwise.
pub(super) fn emit_and_push_called_class_id_if_needed(
    branches: &[StaticPropertyBranch],
    emitter: &mut Emitter,
    ctx: &Context,
) -> bool {
    if branches.is_empty() {
        return false;
    }
    let class_id_reg = class_id_work_reg(emitter);
    if !emit_called_class_id_into(emitter, ctx, class_id_reg) {
        emitter.comment("WARNING: missing forwarded called class id");
        return false;
    }
    abi::emit_push_reg(emitter, class_id_reg);                                  // preserve the called class id across value evaluation
    true
}

/// Loads the called class ID from either `__elephc_called_class_id` or `$this->class_id` into `dest`.
/// Returns `false` if neither is available in the current context.
fn emit_called_class_id_into(emitter: &mut Emitter, ctx: &Context, dest: &str) -> bool {
    if let Some(var) = ctx.variables.get("__elephc_called_class_id") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load the forwarded called-class id from the current static method frame
    } else if let Some(var) = ctx.variables.get("this") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load $this so its runtime class id can drive late static storage
        abi::emit_load_from_address(
            emitter,
            abi::int_result_reg(emitter),
            abi::int_result_reg(emitter),
            0,
        );
    } else {
        return false;
    }
    emitter.instruction(&format!("mov {}, {}", dest, abi::int_result_reg(emitter))); //copy the called class id into a scratch register for branch dispatch
    true
}

/// Emits a conditional load of a static property using late-bound dispatch.
/// Branch label entries are emitted for each `StaticPropertyBranch`; falls back to `fallback_declaring_class` on no match.
pub(super) fn emit_dynamic_load_static_property_reg(
    property: &str,
    class_id_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    dest_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let done = ctx.next_label("static_prop_load_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_load_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    abi::emit_load_symbol_to_reg(emitter, dest_reg, &fallback_symbol, 0);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        if branch.private_inaccessible {
            emit_private_static_property_access_fatal(emitter);
            continue;
        }
        let symbol = static_property_symbol(&branch.declaring_class, property);
        abi::emit_load_symbol_to_reg(emitter, dest_reg, &symbol, 0);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

/// Emits a conditional store to a static property using late-bound dispatch with value in the ABI result register.
/// `release_previous` controls whether the previous static property value is released before storing.
pub(super) fn emit_dynamic_store_result_to_static_property(
    property: &str,
    class_id_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    ty: &PhpType,
    release_previous: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let done = ctx.next_label("static_prop_store_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_store_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    abi::emit_store_result_to_symbol(emitter, &fallback_symbol, ty, release_previous);
    clear_uninitialized_marker_after_static_store(emitter, &fallback_symbol, ty);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        if branch.private_inaccessible {
            emit_private_static_property_access_fatal(emitter);
            continue;
        }
        let symbol = static_property_symbol(&branch.declaring_class, property);
        abi::emit_store_result_to_symbol(emitter, &symbol, ty, release_previous);
        clear_uninitialized_marker_after_static_store(emitter, &symbol, ty);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

/// Emits a conditional store to a static property using late-bound dispatch with value in `source_reg`.
/// Used for array push operations where the value is already materialized in a specific register.
pub(super) fn emit_dynamic_store_reg_to_static_property(
    property: &str,
    class_id_reg: &str,
    source_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let done = ctx.next_label("static_prop_store_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_store_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    abi::emit_store_reg_to_symbol(emitter, source_reg, &fallback_symbol, 0);
    clear_uninitialized_marker_after_static_store(emitter, &fallback_symbol, ty);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        if branch.private_inaccessible {
            emit_private_static_property_access_fatal(emitter);
            continue;
        }
        let symbol = static_property_symbol(&branch.declaring_class, property);
        abi::emit_store_reg_to_symbol(emitter, source_reg, &symbol, 0);
        clear_uninitialized_marker_after_static_store(emitter, &symbol, ty);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

/// Clears the uninitialized marker (a zeroed word) after a static property store.
/// Strings are exempt since they use a separate pointer-plus-length representation.
pub(super) fn clear_uninitialized_marker_after_static_store(
    emitter: &mut Emitter,
    symbol: &str,
    ty: &PhpType,
) {
    if !matches!(ty.codegen_repr(), PhpType::Str) {
        abi::emit_store_zero_to_symbol(emitter, symbol, 8);
    }
}

/// Emits a comparison and conditional branch when `class_id_reg` matches `class_id`, jumping to `label`.
fn emit_branch_if_class_id_matches(
    emitter: &mut Emitter,
    class_id_reg: &str,
    class_id: u64,
    label: &str,
) {
    let compare_reg = class_id_compare_reg(emitter);
    abi::emit_load_int_immediate(emitter, compare_reg, class_id as i64);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); //compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("b.eq {}", label));                    // use this static property slot when the called class id matches
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); //compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("je {}", label));                      // use this static property slot when the called class id matches
        }
    }
}

/// Emits an unconditional jump to `label` using the target's native branch instruction.
fn emit_jump(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b {}", label));                       // jump to the end of the static property dispatch chain
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", label));                     // jump to the end of the static property dispatch chain
        }
    }
}

/// Returns the work register for holding a class ID during late-bound dispatch (x13 on ARM64, r13 on x86_64).
pub(super) fn class_id_work_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x13",
        Arch::X86_64 => "r13",
    }
}

/// Returns the scratch register for comparing class IDs (x14 on ARM64, r14 on x86_64).
fn class_id_compare_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x14",
        Arch::X86_64 => "r14",
    }
}

/// Collects all late-bound dispatch branches for a static property access on `receiver`.
/// Only returns branches for classes that are descendants of the base class; skips the fallback declaring class.
/// Private properties on non-declaring descendants are marked as inaccessible and cause a fatal on access.
pub(super) fn dynamic_static_property_branches(
    receiver: &StaticReceiver,
    property: &str,
    fallback_declaring_class: &str,
    ctx: &Context,
) -> Vec<StaticPropertyBranch> {
    if !matches!(receiver, StaticReceiver::Static) {
        return Vec::new();
    }
    let Some(base_class) = ctx.current_class.as_deref() else {
        return Vec::new();
    };
    let mut branches = Vec::new();
    for (class_name, class_info) in &ctx.classes {
        if !is_same_or_descendant(class_name, base_class, ctx) {
            continue;
        }
        let Some(declaring_class) = class_info.static_property_declaring_classes.get(property) else {
            continue;
        };
        if declaring_class == fallback_declaring_class {
            continue;
        }
        let visibility = class_info
            .static_property_visibilities
            .get(property)
            .unwrap_or(&Visibility::Public);
        branches.push(StaticPropertyBranch {
            class_id: class_info.class_id,
            declaring_class: declaring_class.clone(),
            private_inaccessible: matches!(visibility, Visibility::Private)
                && Some(declaring_class.as_str()) != ctx.current_class.as_deref(),
        });
    }
    branches.sort_by_key(|branch| branch.class_id);
    branches.dedup_by_key(|branch| branch.class_id);
    branches
}

/// Returns `true` if `class_name` is the same as or a descendant of `ancestor` in the class hierarchy.
fn is_same_or_descendant(class_name: &str, ancestor: &str, ctx: &Context) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = ctx
            .classes
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Emits a fatal error for an inaccessible private static property access.
/// Writes a message to stderr and terminates the process with exit code 1.
fn emit_private_static_property_access_fatal(emitter: &mut Emitter) {
    let len = STATIC_PROP_PRIVATE_ACCESS_MSG.len();
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for the private static property fatal diagnostic
            abi::emit_symbol_address(emitter, "x1", STATIC_PROP_PRIVATE_ACCESS_LABEL);
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the private static property fatal diagnostic byte length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit status 1 indicates abnormal termination
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", STATIC_PROP_PRIVATE_ACCESS_LABEL); // point the Linux write buffer at the private static property fatal diagnostic
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the private static property fatal diagnostic byte length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr for the private static property fatal diagnostic
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the private static property fatal diagnostic before terminating
            emitter.instruction("mov edi, 1");                                  // exit status 1 indicates abnormal termination
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after reporting the private static property access
        }
    }
}
