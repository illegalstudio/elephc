//! Purpose:
//! Groups object expression lowering for allocation, access, dispatch, static properties, nullsafe, and instanceof.
//! Provides the object-facing API used by the main expression dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::emit_expr()`
//!
//! Key details:
//! - Object results are refcounted handles whose metadata must match class tables and vtable layout.

mod access;
mod allocation;
/// dispatch
pub(crate) mod dispatch;
mod fiber_callable;
mod fiber_wrapper;
mod instanceof;
mod nullsafe;
mod reflection;
mod static_properties;

use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::scalars;
use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, StaticReceiver};
use crate::types::PhpType;

/// Emits `new ClassName(...)` for a known class with constructor args.
pub(crate) fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    allocation::emit_new_object(class_name, args, emitter, ctx, data)
}

/// Emits a `new $class(...)`-style internal factory constrained to a parent class.
pub(crate) fn emit_new_dynamic_object(
    class_name: &Expr,
    fallback_class: &str,
    required_parent: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!(
        "new dynamic {} subclass from class-string",
        required_parent
    ));
    let class_ty = super::emit_expr(class_name, emitter, ctx, data).codegen_repr();
    if !emit_prepare_dynamic_new_class_string(&class_ty, required_parent, emitter, ctx, data) {
        return PhpType::Object(fallback_class.to_string());
    }

    abi::emit_call_label(emitter, "__rt_instanceof_lookup");                    // resolve the requested dynamic factory class-string to class metadata
    let invalid_label = ctx.next_label("dynamic_new_invalid");
    let unmatched_label = ctx.next_label("dynamic_new_unmatched");
    let done_label = ctx.next_label("dynamic_new_done");
    emit_branch_if_dynamic_new_lookup_invalid(&invalid_label, emitter);
    emit_push_dynamic_new_class_id(emitter);

    let classes = sorted_dynamic_new_classes_by_id(required_parent, ctx);
    let mut cases = Vec::new();
    for (_, class_id) in &classes {
        let label = ctx.next_label("dynamic_new_case");
        emit_compare_dynamic_new_class_id(*class_id, &label, emitter);
        cases.push(label);
    }
    abi::emit_jump(emitter, &unmatched_label);                                  // report invalid factory classes that are outside the required parent hierarchy

    emitter.label(&unmatched_label);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the unmatched resolved class id before aborting
    emit_dynamic_new_fatal(required_parent, emitter, data);

    emitter.label(&invalid_label);
    emit_dynamic_new_fatal(required_parent, emitter, data);

    for ((class_name, _), label) in classes.into_iter().zip(cases) {
        emitter.label(&label);
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the resolved class id before constructing the selected class
        allocation::emit_new_object(&class_name, args, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);                                   // skip the remaining dynamic factory cases after construction
    }

    emitter.label(&done_label);
    PhpType::Object(fallback_class.to_string())
}

/// Normalizes a direct or boxed class-string into the ABI string-result registers.
fn emit_prepare_dynamic_new_class_string(
    class_ty: &PhpType,
    required_parent: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> bool {
    match class_ty {
        PhpType::Str => true,
        PhpType::Mixed | PhpType::Union(_) => {
            let ok_label = ctx.next_label("dynamic_new_class_string");
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // unwrap nullable/mixed factory class names before class metadata lookup
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #1");                          // runtime tag 1 means the factory argument is a string
                    emitter.instruction(&format!("b.eq {}", ok_label));         // continue only when the boxed factory argument is a class-string
                    emit_dynamic_new_fatal(required_parent, emitter, data);
                    emitter.label(&ok_label);
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 1");                          // runtime tag 1 means the factory argument is a string
                    emitter.instruction(&format!("je {}", ok_label));           // continue only when the boxed factory argument is a class-string
                    emit_dynamic_new_fatal(required_parent, emitter, data);
                    emitter.label(&ok_label);
                    emitter.instruction("mov rax, rdi");                        // move the unboxed string pointer into the lookup input register
                }
            }
            true
        }
        _ => {
            emit_dynamic_new_fatal(required_parent, emitter, data);
            false
        }
    }
}

/// Emits a dynamic property access where the property name is a runtime expression.
pub(crate) fn emit_dynamic_property_access(
    object: &Expr,
    property: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_dynamic_property_access(object, property, false, emitter, ctx, data)
}

/// Emits a nullsafe dynamic property access (`?->`).
pub(crate) fn emit_nullsafe_dynamic_property_access(
    object: &Expr,
    property: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_dynamic_property_access(object, property, true, emitter, ctx, data)
}

/// Emits a property access on a `Mixed`-typed receiver by name.
pub(crate) fn emit_mixed_property_access(
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_mixed_property_access(property, emitter, ctx, data)
}

/// Resolves a `StaticReceiver` (`self`/`parent`/`Named`) to a class name string.
/// Returns `None` for `Static` (late-bound) which must be handled at runtime.
fn resolve_scoped_receiver_to_class(receiver: &StaticReceiver, ctx: &Context) -> Option<String> {
    match receiver {
        StaticReceiver::Self_ => ctx.current_class.clone(),
        StaticReceiver::Parent => ctx
            .current_class
            .as_ref()
            .and_then(|c| ctx.classes.get(c))
            .and_then(|info| info.parent.clone()),
        StaticReceiver::Named(name) => Some(name.as_canonical()),
        StaticReceiver::Static => None,
    }
}

/// Emits a class constant access for `self`/`parent`/`Named` receivers.
/// For `Static` receivers, dispatches to `emit_late_bound_class_constant` at runtime.
pub(super) fn emit_class_constant(
    receiver: &StaticReceiver,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if matches!(receiver, StaticReceiver::Static) {
        return emit_late_bound_class_constant(emitter, ctx, data);
    }

    let name = resolve_scoped_receiver_to_class(receiver, ctx).unwrap_or_default();
    scalars::emit_string_literal(&name, emitter, data)
}

/// Emits a scoped constant access (self/parent/named receiver with constant name).
pub(super) fn emit_scoped_constant_access(
    receiver: &StaticReceiver,
    name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let class_name = resolve_scoped_receiver_to_class(receiver, ctx)
        .expect("ScopedConstantAccess on `static` not supported yet");
    // Enum case: dispatch to the existing enum codegen.
    if ctx.enums.contains_key(&class_name) {
        return emit_enum_case(&class_name, name, emitter, ctx);
    }
    // Class constant: walk parent chain.
    let mut current: Option<String> = Some(class_name.clone());
    let mut value: Option<Expr> = None;
    while let Some(cn) = current.as_deref() {
        if let Some(info) = ctx.classes.get(cn) {
            if let Some(v) = info.constants.get(name).cloned() {
                value = Some(v);
                break;
            }
            current = info.parent.clone();
        } else {
            break;
        }
    }
    if value.is_none() {
        // Search interfaces (and parent interfaces) the class implements.
        let mut visited: std::collections::HashSet<String> = Default::default();
        let mut queue: Vec<String> = ctx
            .classes
            .get(&class_name)
            .map(|info| info.interfaces.clone())
            .unwrap_or_default();
        // Direct interface receiver: include the receiver itself.
        queue.push(class_name.clone());
        while let Some(iface_name) = queue.pop() {
            if !visited.insert(iface_name.clone()) {
                continue;
            }
            if let Some(info) = ctx.interfaces.get(&iface_name) {
                if let Some(v) = info.constants.get(name).cloned() {
                    value = Some(v);
                    break;
                }
                queue.extend(info.parents.iter().cloned());
            }
        }
    }
    let value = value.expect("type checker rejected unresolved class constant");
    super::emit_expr(&value, emitter, ctx, data)
}

/// Emits `new self/parent/Static(...)` with a late-bound class.
pub(super) fn emit_new_scoped_object(
    receiver: &StaticReceiver,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if matches!(receiver, StaticReceiver::Static) {
        return emit_late_bound_new_static(args, emitter, ctx, data);
    }

    let class_name = resolve_scoped_receiver_to_class(receiver, ctx)
        .expect("new self/parent/static used outside class context — should be a type error");
    allocation::emit_new_object(&class_name, args, emitter, ctx, data)
}

/// Collects all classes in the current inheritance hierarchy (same class or descendants)
/// sorted by class ID, used for late-static-binding dispatch tables.
fn sorted_late_bound_classes_by_id(ctx: &Context) -> Vec<(String, u64)> {
    let Some(base_class) = ctx.current_class.as_deref() else {
        return Vec::new();
    };
    let mut classes: Vec<(String, u64)> = ctx
        .classes
        .iter()
        .filter(|(name, _)| class_is_same_or_descends_from(name, base_class, ctx))
        .map(|(name, info)| (name.clone(), info.class_id))
        .collect();
    classes.sort_by_key(|(_, class_id)| *class_id);
    classes
}

/// Returns true if `class_name` is the same as `base_class` or descends from it.
fn class_is_same_or_descends_from(class_name: &str, base_class: &str, ctx: &Context) -> bool {
    let mut current = Some(class_name);
    while let Some(name) = current {
        if class_names_match(name, base_class) {
            return true;
        }
        current = ctx.classes.get(name).and_then(|info| info.parent.as_deref());
    }
    false
}

/// Compares PHP class names using the same case-insensitive key used by symbol tables.
fn class_names_match(left: &str, right: &str) -> bool {
    php_symbol_key(left.trim_start_matches('\\')) == php_symbol_key(right.trim_start_matches('\\'))
}

/// Collects all concrete dynamic factory targets that satisfy the required parent.
fn sorted_dynamic_new_classes_by_id(
    required_parent: &str,
    ctx: &Context,
) -> Vec<(String, u64)> {
    let mut classes: Vec<(String, u64)> = ctx
        .classes
        .iter()
        .filter(|(name, _)| class_is_same_or_descends_from(name, required_parent, ctx))
        .map(|(name, info)| (name.clone(), info.class_id))
        .collect();
    classes.sort_by_key(|(_, class_id)| *class_id);
    classes
}

/// Branches when the dynamic factory class-string lookup failed or resolved to an interface.
fn emit_branch_if_dynamic_new_lookup_invalid(invalid_label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // did the dynamic factory class-string resolve to metadata?
            emitter.instruction(&format!("b.eq {}", invalid_label));            // abort unresolved factory classes before constructor arguments are evaluated
            emitter.instruction("cmp x2, #0");                                  // target kind 0 means a concrete class, not an interface
            emitter.instruction(&format!("b.ne {}", invalid_label));            // abort interface targets because factories must instantiate objects
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // did the dynamic factory class-string resolve to metadata?
            emitter.instruction(&format!("je {}", invalid_label));              // abort unresolved factory classes before constructor arguments are evaluated
            emitter.instruction("test rdx, rdx");                               // target kind 0 means a concrete class, not an interface
            emitter.instruction(&format!("jne {}", invalid_label));             // abort interface targets because factories must instantiate objects
        }
    }
}

/// Preserves the resolved dynamic factory class id on the temporary stack.
fn emit_push_dynamic_new_class_id(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_push_reg(emitter, "x1"),
        Arch::X86_64 => abi::emit_push_reg(emitter, "rdi"),
    }
}

/// Compares the saved dynamic factory class id with a concrete candidate class.
fn emit_compare_dynamic_new_class_id(
    class_id: u64,
    matched_label: &str,
    emitter: &mut Emitter,
) {
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_temporary_stack_slot(emitter, scratch, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", scratch, class_id));    // compare the requested factory class with this concrete class id
            emitter.instruction(&format!("b.eq {}", matched_label));            // branch when the runtime class-string selected this constructor
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", scratch, class_id));     // compare the requested factory class with this concrete class id
            emitter.instruction(&format!("je {}", matched_label));              // branch when the runtime class-string selected this constructor
        }
    }
}

/// Emits a fatal diagnostic for invalid dynamic SPL factory class names.
fn emit_dynamic_new_fatal(required_parent: &str, emitter: &mut Emitter, data: &mut DataSection) {
    let message = format!(
        "Fatal error: Dynamic factory class must extend {}\n",
        required_parent
    );
    let (message_label, message_len) = data.add_string(message.as_bytes());
    emit_fatal_message(emitter, &message_label, message_len);
}

/// Unboxes a Mixed value and emits a fatal if it is null instead of an object.
pub(crate) fn emit_unbox_mixed_object_or_fatal(
    message: &[u8],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let (message_label, message_len) = data.add_string(message);
    let ok_label = ctx.next_label("mixed_object_not_null");
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // inspect the boxed nullable object before member access
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #8");                                  // runtime tag 8 means the nullable receiver is null
            emitter.instruction(&format!("b.ne {}", ok_label));                 // continue only for a real object payload
            emit_fatal_message(emitter, &message_label, message_len);
            emitter.label(&ok_label);
            emitter.instruction("mov x0, x1");                                  // promote the unboxed object pointer into the AArch64 result register
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 8");                                  // runtime tag 8 means the nullable receiver is null
            emitter.instruction(&format!("jne {}", ok_label));                  // continue only for a real object payload
            emit_fatal_message(emitter, &message_label, message_len);
            emitter.label(&ok_label);
            emitter.instruction("mov rax, rdi");                                // promote the unboxed object pointer into the SysV result register
        }
    }
}

/// Emits a null-check branch on a Mixed-object unbox result for nullsafe flows.
pub(super) fn emit_unbox_mixed_object_or_null_branch(null_label: &str, emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // inspect the boxed nullable object before member access
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #8");                                  // runtime tag 8 means the nullable receiver is null
            emitter.instruction(&format!("b.eq {}", null_label));               // branch to the PHP null receiver path instead of dereferencing it
            emitter.instruction("mov x0, x1");                                  // promote the unboxed object pointer into the AArch64 result register
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 8");                                  // runtime tag 8 means the nullable receiver is null
            emitter.instruction(&format!("je {}", null_label));                 // branch to the PHP null receiver path instead of dereferencing it
            emitter.instruction("mov rax, rdi");                                // promote the unboxed object pointer into the SysV result register
        }
    }
}

/// Emits a runtime warning diagnostic with the given message.
pub(super) fn emit_runtime_warning(
    message: &[u8],
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (message_label, message_len) = data.add_string(message);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", &message_label);                                 // load the page containing the runtime warning text
            emitter.add_lo12("x1", "x1", &message_label);                       // resolve the runtime warning text address
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the runtime warning byte length to the diagnostic helper
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", &message_label);           // pass the runtime warning text pointer to the diagnostic helper
            emitter.instruction(&format!("mov esi, {}", message_len));          // pass the runtime warning byte length to the diagnostic helper
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the runtime warning under the current @ scope
}

/// Emits a boxed null value (tagged nullable pointer) into expression result registers.
pub(super) fn emit_boxed_null(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        0x7fff_ffff_ffff_fffe,
    );
    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Void);
}

/// Boxes the current expression result as Mixed if the result type is not already Mixed.
pub(super) fn box_nullable_result(result_ty: &PhpType, emitter: &mut Emitter) {
    if !matches!(result_ty.codegen_repr(), PhpType::Mixed) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, result_ty);
    }
}

/// Emits the fatal-message sequence (write to stderr + exit) for null object derefs.
fn emit_fatal_message(emitter: &mut Emitter, message_label: &str, message_len: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for the nullable-object fatal diagnostic
            emitter.adrp("x1", message_label);                                  // load the page containing the nullable-object fatal diagnostic
            emitter.add_lo12("x1", "x1", message_label);                        // resolve the nullable-object fatal diagnostic address
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the nullable-object fatal diagnostic length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit status 1 indicates abnormal termination
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", message_label);            // point the Linux write buffer at the nullable-object fatal diagnostic
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the nullable-object fatal diagnostic length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr for the nullable-object fatal diagnostic
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the nullable-object fatal diagnostic
            emitter.instruction("mov edi, 1");                                  // exit status 1 indicates abnormal termination
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate after reporting the nullable-object fatal diagnostic
        }
    }
}

/// Emits the forwarded called-class ID or falls back to the lexical current-class ID.
fn emit_late_bound_class_id_or_lexical_fallback(emitter: &mut Emitter, ctx: &Context) {
    if !dispatch::emit_forwarded_called_class_id(emitter, ctx) {
        let class_id = ctx
            .current_class
            .as_ref()
            .and_then(|name| ctx.classes.get(name))
            .map(|info| info.class_id)
            .unwrap_or(0);
        dispatch::emit_immediate_class_id(emitter, class_id);
    }
}

/// Emits a comparison of the forwarded called-class ID against a concrete class ID,
/// branching to `matched_label` if they match.
fn emit_compare_current_class_id(emitter: &mut Emitter, class_id: u64, matched_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x0, #{}", class_id));             // compare the forwarded called-class id against this concrete class id
            emitter.instruction(&format!("b.eq {}", matched_label));            // branch to the matching late-static-binding case
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rax, {}", class_id));             // compare the forwarded called-class id against this concrete class id
            emitter.instruction(&format!("je {}", matched_label));              // branch to the matching late-static-binding case
        }
    }
}

/// Emits a late-bound class constant using the forwarded called-class ID,
/// branching to the matching class's constant, with a lexical fallback.
fn emit_late_bound_class_constant(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let classes = sorted_late_bound_classes_by_id(ctx);
    let done_label = ctx.next_label("static_class_done");
    let fallback_name = ctx.current_class.clone().unwrap_or_default();

    emit_late_bound_class_id_or_lexical_fallback(emitter, ctx);
    let mut cases = Vec::new();
    for (_, class_id) in &classes {
        let label = ctx.next_label("static_class_case");
        emit_compare_current_class_id(emitter, *class_id, &label);
        cases.push(label);
    }

    scalars::emit_string_literal(&fallback_name, emitter, data);
    abi::emit_jump(emitter, &done_label);                                       // skip late-static-binding class-name cases after using the lexical fallback

    for ((class_name, _), label) in classes.into_iter().zip(cases) {
        emitter.label(&label);
        scalars::emit_string_literal(&class_name, emitter, data);
        abi::emit_jump(emitter, &done_label);                                   // finish after materializing the matched late-bound class name
    }

    emitter.label(&done_label);
    PhpType::Str
}

/// Emits a `new static(...)` call using the forwarded called-class ID,
/// branching to the matching class's constructor, with a lexical fallback.
fn emit_late_bound_new_static(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let classes = sorted_late_bound_classes_by_id(ctx);
    let done_label = ctx.next_label("new_static_done");
    let fallback_class = ctx.current_class.clone().unwrap_or_default();

    emit_late_bound_class_id_or_lexical_fallback(emitter, ctx);
    let mut cases = Vec::new();
    for (_, class_id) in &classes {
        let label = ctx.next_label("new_static_case");
        emit_compare_current_class_id(emitter, *class_id, &label);
        cases.push(label);
    }

    if !fallback_class.is_empty() {
        allocation::emit_new_object(&fallback_class, args, emitter, ctx, data);
    }
    abi::emit_jump(emitter, &done_label);                                       // skip concrete new-static cases after the lexical fallback

    for ((class_name, _), label) in classes.into_iter().zip(cases) {
        emitter.label(&label);
        allocation::emit_new_object(&class_name, args, emitter, ctx, data);
        abi::emit_jump(emitter, &done_label);                                   // finish after constructing the matched late-bound class
    }

    emitter.label(&done_label);
    PhpType::Object(fallback_class)
}

/// Emits a direct property access on a known class with a literal property name.
pub(super) fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_property_access(object, property, emitter, ctx, data)
}

/// Emits a property access on a nullable class where the class is known at codegen time.
pub(super) fn emit_nullable_object_property_access(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_nullable_object_property_access(class_name, property, emitter, ctx, data)
}

/// Emits a property access where the class is known but property is dynamically loaded.
pub(super) fn emit_loaded_object_property_access(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    access::emit_loaded_object_property_access(class_name, property, emitter, ctx, data)
}

/// Emits a nullsafe property access (`?.property`).
pub(super) fn emit_nullsafe_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    nullsafe::emit_nullsafe_property_access(object, property, emitter, ctx, data)
}

/// Emits a static property access (`StaticClass::$property`).
pub(super) fn emit_static_property_access(
    receiver: &StaticReceiver,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    static_properties::emit_static_property_access(receiver, property, emitter, ctx, data)
}

/// Emits a `ClassName::Case` enum case singleton load.
pub(super) fn emit_enum_case(
    enum_name: &str,
    case_name: &str,
    emitter: &mut Emitter,
    _ctx: &mut Context,
) -> PhpType {
    let label = crate::names::enum_case_symbol(enum_name, case_name);
    emitter.comment(&format!("load enum case {}::{}", enum_name, case_name));
    crate::codegen::abi::emit_load_symbol_to_reg(
        emitter,
        crate::codegen::abi::int_result_reg(emitter),
        &label,
        0,
    ); // load the enum singleton pointer from its global slot through the target-aware symbol helper
    PhpType::Object(enum_name.to_string())
}

/// Pushes a magic `__property` name as a string argument pair for `__get`/`__set` calls.
pub(crate) fn push_magic_property_name_arg(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (label, len) = data.add_string(property.as_bytes());
    let (ptr_reg, len_reg) = crate::codegen::abi::string_result_regs(emitter);
    crate::codegen::abi::emit_symbol_address(emitter, ptr_reg, &label); // materialize the magic-property name string address for the active target ABI
    crate::codegen::abi::emit_load_int_immediate(emitter, len_reg, len as i64); // materialize the magic-property name length for the active target ABI
    crate::codegen::abi::emit_push_reg_pair(emitter, ptr_reg, len_reg); // push the magic-property name argument pair onto the temporary call stack
}

/// Returns `[method_name_string, args_array]` for `__call`/`__callStatic` magic dispatch.
pub(super) fn magic_method_args(method: &str, args: &[Expr], span: crate::span::Span) -> Vec<Expr> {
    vec![
        Expr::new(ExprKind::StringLiteral(method.to_string()), span),
        Expr::new(ExprKind::ArrayLiteral(args.to_vec()), span),
    ]
}

/// Emits an instance method call (`$object->method(...)`).
pub(crate) fn emit_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    dispatch::emit_method_call(object, method, args, emitter, ctx, data)
}

/// Emits a nullsafe method call (`?->method(...)`).
pub(super) fn emit_nullsafe_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    nullsafe::emit_nullsafe_method_call(object, method, args, emitter, ctx, data)
}

/// Emits a method call on a known class with args already pushed to the stack.
pub(crate) fn emit_method_call_with_pushed_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    dispatch::emit_method_call_with_pushed_args(class_name, method, arg_types, 0, emitter, ctx)
}

/// Emits a method call with the receiver saved below the pushed args on the stack.
pub(super) fn emit_method_call_with_saved_receiver_below_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    source_temp_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    dispatch::emit_method_call_with_saved_receiver_below_args(
        class_name,
        method,
        arg_types,
        source_temp_bytes,
        emitter,
        ctx,
    )
}

/// Emits the args portion of a method call when args have already been pushed.
pub(super) fn emit_pushed_method_args(
    args: &[Expr],
    sig: Option<&crate::types::FunctionSig>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> super::calls::args::EmittedCallArgs {
    dispatch::emit_pushed_method_args(args, sig, emitter, ctx, data)
}

/// Emits a static method call (`ClassName::method(...)` or `self/parent/static`).
pub(crate) fn emit_static_method_call(
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    dispatch::emit_static_method_call(receiver, method, args, emitter, ctx, data)
}

/// Emits an instanceof type check expression.
pub(super) fn emit_instanceof(
    value: &Expr,
    target: &InstanceOfTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    instanceof::emit_instanceof(value, target, emitter, ctx, data)
}
