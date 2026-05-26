//! Purpose:
//! Lowers static property reads with late-bound receiver handling.
//! Produces object-related expression results while respecting runtime metadata and ownership rules.
//!
//! Called from:
//! - `crate::codegen::expr::objects`
//!
//! Key details:
//! - Object handles, property storage, and class ids must stay consistent with emitted class tables.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::names::static_property_symbol;
use crate::parser::ast::{StaticReceiver, Visibility};
use crate::types::PhpType;

const STATIC_PROP_PRIVATE_ACCESS_LABEL: &str = "_static_prop_private_access_msg";
const STATIC_PROP_PRIVATE_ACCESS_MSG: &str =
    "Fatal error: Cannot access private static property\n";

/// A single dispatch branch for a redeclared static property.
/// Tracks the runtime class id, the class that actually declares the property,
/// and whether the current context is forbidden from accessing it (private visibility).
#[derive(Clone)]
struct StaticPropertyBranch {
    class_id: u64,
    declaring_class: String,
    private_inaccessible: bool,
}

/// Emits the static property access sequence for a given receiver and property name.
///
/// Late-bound receiver resolution uses the forwarded `__elephc_called_class_id` or `$this`
/// to determine the declaring class at runtime when `StaticReceiver::Static` is used.
/// For static receivers without redeclarations, emits a direct symbol load.
/// Returns the declared `PhpType` of the property.
pub(super) fn emit_static_property_access(
    receiver: &StaticReceiver,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let Some((class_name, declaring_class, prop_ty)) =
        resolve_static_property(receiver, property, ctx, emitter)
    else {
        return PhpType::Int;
    };

    emitter.comment(&format!("{}::${}", class_name, property));
    let branches = dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    if branches.is_empty() {
        let symbol = static_property_symbol(&declaring_class, property);
        if static_property_has_declared_type(&declaring_class, property, ctx) {
            emit_uninitialized_static_property_guard(&declaring_class, property, &symbol, emitter, ctx, data);
        }
        abi::emit_load_symbol_to_result(emitter, &symbol, &prop_ty);
    } else if emit_called_class_id_into(emitter, ctx, class_id_work_reg(emitter)) {
        emit_dynamic_load_static_property_result(
            property,
            class_id_work_reg(emitter),
            &declaring_class,
            &branches,
            &prop_ty,
            emitter,
            ctx,
            data,
        );
    } else {
        emitter.comment("WARNING: missing forwarded called class id");
        let symbol = static_property_symbol(&declaring_class, property);
        if static_property_has_declared_type(&declaring_class, property, ctx) {
            emit_uninitialized_static_property_guard(&declaring_class, property, &symbol, emitter, ctx, data);
        }
        abi::emit_load_symbol_to_result(emitter, &symbol, &prop_ty);
    }
    prop_ty
}

/// Emits a branch-dispatch sequence for late-bound static property access.
///
/// Generates an if-else chain: one branch per redeclared static property owner
/// (ordered by class id), plus a fallback to `fallback_declaring_class`.
/// Each branch checks the uninitialized sentinel for typed properties before loading.
/// Terminates with a fatal if a private property is inaccessible from the current context.
fn emit_dynamic_load_static_property_result(
    property: &str,
    class_id_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    prop_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let done = ctx.next_label("static_prop_load_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_load_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    if static_property_has_declared_type(fallback_declaring_class, property, ctx) {
        emit_uninitialized_static_property_guard(
            fallback_declaring_class,
            property,
            &fallback_symbol,
            emitter,
            ctx,
            data,
        );
    }
    abi::emit_load_symbol_to_result(emitter, &fallback_symbol, prop_ty);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        if branch.private_inaccessible {
            emit_private_static_property_access_fatal(emitter);
            continue;
        }
        let symbol = static_property_symbol(&branch.declaring_class, property);
        if static_property_has_declared_type(&branch.declaring_class, property, ctx) {
            emit_uninitialized_static_property_guard(
                &branch.declaring_class,
                property,
                &symbol,
                emitter,
                ctx,
                data,
            );
        }
        abi::emit_load_symbol_to_result(emitter, &symbol, prop_ty);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

/// Loads the late-bound "called class id" into `dest` and returns `true`.
///
/// Preference order:
/// 1. `__elephc_called_class_id` variable (forwarded from static method frame)
/// 2. `$this` variable (use its runtime class id for late static binding)
/// 3. Neither available → returns `false` and `dest` is unchanged.
///
/// Used by dynamic static property dispatch to determine which class's
/// redeclared property slot to access at runtime.
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
    emitter.instruction(&format!("mov {}, {}", dest, abi::int_result_reg(emitter))); // copy the called class id into a scratch register for branch dispatch
    true
}

/// Emits a compare-and-branch instruction for a specific class id.
///
/// Compares `class_id_reg` (runtime called class id) against `class_id`.
/// On AArch64 emits `cmp`/`b.eq`; on x86_64 emits `cmp`/`je`.
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
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("b.eq {}", label));                    // read this static property slot when the called class id matches
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("je {}", label));                      // read this static property slot when the called class id matches
        }
    }
}

/// Emits an unconditional jump to `label`.
/// AArch64 uses `b`; x86_64 uses `jmp`.
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

/// Returns the scratch register used to hold a class id during static property dispatch.
/// AArch64: `x13`; x86_64: `r13`.
fn class_id_work_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x13",
        Arch::X86_64 => "r13",
    }
}

/// Returns the scratch register used to hold the immediate class id in branch comparisons.
/// AArch64: `x14`; x86_64: `r14`.
fn class_id_compare_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x14",
        Arch::X86_64 => "r14",
    }
}

/// Collects all redeclared static property branches for a late-bound static receiver.
///
/// Only applies when `receiver` is `StaticReceiver::Static` and the current class context
/// is set. Walks the class hierarchy from the current class, collecting descendants that
/// redeclare the property with a different declaring class. Private properties that the
/// current context cannot access are marked `private_inaccessible`.
/// Returns branches sorted and deduplicated by `class_id`.
fn dynamic_static_property_branches(
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

/// Returns `true` if `class_name` is `ancestor` or a descendant of it.
/// Walks the parent chain via `ctx.classes`.
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

/// Resolves a static property receiver and property name to class metadata.
///
/// Translates `StaticReceiver` variants to concrete class names:
/// - `Named` → the specified class
/// - `Self_` / `Static` → the current class from `ctx`
/// - `Parent` → the parent of the current class
///
/// Returns `None` if no class context is available, the class is undefined,
/// or the property does not exist on that class. On success returns
/// `(class_name, declaring_class, prop_ty)`.
pub(crate) fn resolve_static_property(
    receiver: &StaticReceiver,
    property: &str,
    ctx: &Context,
    emitter: &mut Emitter,
) -> Option<(String, String, PhpType)> {
    let class_name = match receiver {
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
        StaticReceiver::Self_ | StaticReceiver::Static => match &ctx.current_class {
            Some(class_name) => class_name.clone(),
            None => {
                emitter.comment("WARNING: self::/static:: used outside class scope");
                return None;
            }
        },
        StaticReceiver::Parent => {
            let current_class = match &ctx.current_class {
                Some(class_name) => class_name.clone(),
                None => {
                    emitter.comment("WARNING: parent:: used outside class scope");
                    return None;
                }
            };
            match ctx.classes.get(&current_class).and_then(|info| info.parent.clone()) {
                Some(parent_name) => parent_name,
                None => {
                    emitter.comment(&format!("WARNING: class {} has no parent", current_class));
                    return None;
                }
            }
        }
    };

    let class_info = match ctx.classes.get(&class_name) {
        Some(class_info) => class_info,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return None;
        }
    };
    let prop_ty = match class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
    {
        Some(prop_ty) => prop_ty,
        None => {
            emitter.comment(&format!(
                "WARNING: undefined static property {}::${}",
                class_name, property
            ));
            return None;
        }
    };
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    Some((class_name, declaring_class, prop_ty))
}

/// Emits a fatal error and terminates the process when a private static property
/// cannot be accessed due to visibility rules.
///
/// Writes a fixed diagnostic string to stderr and exits with status 1.
/// Target-specific: AArch64 uses `adrp`/`add-lo12` + syscall; x86_64 uses direct register setup + syscall.
fn emit_private_static_property_access_fatal(emitter: &mut Emitter) {
    let len = STATIC_PROP_PRIVATE_ACCESS_MSG.len();
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for the private static property fatal diagnostic
            emitter.adrp("x1", STATIC_PROP_PRIVATE_ACCESS_LABEL);
            emitter.add_lo12("x1", "x1", STATIC_PROP_PRIVATE_ACCESS_LABEL);
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

/// Returns `true` if `declaring_class` has an explicit declared type for `property`.
///
/// Checks whether `property` appears in `class_info.declared_static_properties`,
/// indicating a typed static property that requires the uninitialized sentinel guard.
fn static_property_has_declared_type(
    declaring_class: &str,
    property: &str,
    ctx: &Context,
) -> bool {
    ctx.classes
        .get(declaring_class)
        .is_some_and(|class_info| class_info.declared_static_properties.contains(property))
}

/// Emits a guard that checks the typed-property sentinel before loading.
///
/// Loads the sentinel value and compares it against the property's current value.
/// Skips the load and jumps to `initialized_label` if the property is already initialized.
/// Otherwise calls `emit_uninitialized_static_property_fatal` and terminates.
fn emit_uninitialized_static_property_guard(
    class_name: &str,
    property: &str,
    symbol: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let initialized_label = ctx.next_label("static_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, marker_reg, symbol, 8);
    abi::emit_load_int_immediate(emitter, sentinel_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // check whether the static typed property is still uninitialized
            emitter.instruction(&format!("b.ne {}", initialized_label));        // continue the static property read once initialized
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // check whether the static typed property is still uninitialized
            emitter.instruction(&format!("jne {}", initialized_label));         // continue the static property read once initialized
        }
    }
    emit_uninitialized_static_property_fatal(class_name, property, emitter, data);
    emitter.label(&initialized_label);
}

/// Emits a fatal error and terminates the process when a typed static property
/// is accessed before initialization.
///
/// Formats the diagnostic message with the class and property name,
/// adds it to the data section as a static string, writes it to stderr,
/// and exits with status 1. Target-specific register sequences apply.
fn emit_uninitialized_static_property_fatal(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let message = format!(
        "Fatal error: Typed static property {}::${} must not be accessed before initialization\n",
        class_name, property
    );
    let (label, len) = data.add_string(message.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for the static typed-property initialization fatal
            abi::emit_symbol_address(emitter, "x1", &label);                    // point write() at the static typed-property diagnostic
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the diagnostic byte length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit status 1 indicates abnormal termination
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", &label);                   // point write() at the static typed-property diagnostic
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the diagnostic byte length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr for the static typed-property initialization fatal
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal diagnostic before terminating
            emitter.instruction("mov edi, 1");                                  // exit status 1 indicates abnormal termination
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate after the static typed-property initialization fatal
        }
    }
}
