//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician access properties on
//! native objects using the current module's class metadata.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object cannot know user class ids or property
//!   offsets, so these C-ABI symbols are emitted into the user assembly.
//! - Supported slots include public properties plus protected/private
//!   properties when the active eval class scope satisfies PHP visibility.
//! - Setters borrow their boxed input; property hashes acquire independent references.

use std::collections::BTreeMap;

use crate::codegen::data_section::DataSection;
use crate::codegen::eval_callable_helpers::{
    emit_aarch64_cast_eval_callable_arg, emit_x86_64_cast_eval_callable_arg,
    EvalCallableDescriptorSupport,
};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime_value_tag;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::codegen::{abi, emit_box_current_value_as_mixed};
use super::class_id_dispatch::{emit_class_id_switch, ClassIdSwitch};
use crate::ir::Module;
use crate::names::join_php_symbol;
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

/// Where the property-SET helper keeps the boxed value, as `[x29 + N]` for the callable cast.
const AARCH64_SET_VALUE_FROM_FP_OFFSET: i64 = -56;
/// Where the property-SET helper keeps the active eval context, as `[x29 + N]`.
const AARCH64_SET_CONTEXT_FROM_FP_OFFSET: i64 = -24;
/// The x86_64 twin of the boxed value, as a `[rbp - N]` displacement.
const X86_64_SET_VALUE_FRAME_OFFSET: usize = 32;
/// The x86_64 twin of the eval context, as a `[rbp - N]` displacement.
const X86_64_SET_CONTEXT_FRAME_OFFSET: usize = 64;
mod dynamic_properties;
mod private_shadow;
mod replacement;
mod unset;
mod unset_boundary;
use replacement::emit_owned_slot_replacement;
use dynamic_properties::{
    emit_dynamic_property_get_fallback, emit_dynamic_property_set_fallback,
    emit_property_hash_slot_helper,
};

/// Property slot metadata needed by eval property bridge dispatch.
#[derive(Clone)]
struct EvalPropertySlot {
    class_id: u64,
    class_name: String,
    declaring_class: String,
    allowed_scopes: Vec<String>,
    property: String,
    visibility: Visibility,
    offset: usize,
    ty: PhpType,
    is_declared: bool,
    is_reference: bool,
    is_hidden_shadow: bool,
}

impl EvalPropertySlot {
    /// Returns the byte offset of the typed-uninitialized marker, relative to the base the body
    /// addresses: the second slot word for a plain slot, the value word of a reference cell.
    fn marker_offset(&self) -> usize {
        if self.is_reference {
            self.offset
        } else {
            self.offset + 8
        }
    }

    /// Returns the slot as the bridge body must address it once it holds the managed cell.
    ///
    /// A reference property keeps a cell pointer where a plain slot keeps the value. The cell's
    /// payload is the slot's own layout at offset 0 (value word, then the second word), and its
    /// typed-uninitialized marker is the VALUE word. Every body therefore replaces the object
    /// pointer in its frame with the cell pointer (`emit_*_enter_reference_cell`) and addresses
    /// this view, which differs from the slot only in its offset.
    fn cell_view(&self) -> EvalPropertySlot {
        let mut view = self.clone();
        if view.is_reference {
            view.offset = 0;
        }
        view
    }

    /// Returns whether a failed scope check leaves the name to dynamic-property storage.
    ///
    /// php mangles a private property into its declaring class's own table entry, so a name the
    /// layout carries only as a STRICT ancestor's private slot is unreachable by name from every
    /// other scope — and php resolves it to a DYNAMIC property there, exactly as
    /// `crate::types::property_name_shadows_ancestor_private_slot` decides for generated code.
    /// Refusing instead turned `$o->hidden = …` in eval into a fatal on a child that merely
    /// INHERITS `private $hidden`, where php creates the child's own property.
    ///
    /// A private the receiver's OWN class declares keeps failing closed: that is php's
    /// `Cannot access private property` path, and it is the case the eval-child probe covers.
    fn scope_miss_is_dynamic(&self) -> bool {
        self.is_hidden_shadow
            || (self.visibility == Visibility::Private
                && self.declaring_class.trim_start_matches('\\')
                    != self.class_name.trim_start_matches('\\'))
    }
}

/// Returns whether the fixed slot can carry the untyped-property removed marker.
fn slot_supports_untyped_unset_marker(slot: &EvalPropertySlot) -> bool {
    !slot.is_declared && !slot.is_reference && slot.ty.codegen_repr() == PhpType::Mixed
}

/// Emits eval property helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_property_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if !module.required_runtime_features.eval_bridge {
        return;
    }
    let slots = collect_eval_property_slots(module);
    let scope_sets = index_scope_sets(&slots);
    emit_property_get_helper(module, emitter, data, &slots, &scope_sets);
    emit_property_is_initialized_helper(module, emitter, data, &slots, &scope_sets);
    emit_property_set_helper(module, emitter, data, &slots, callable_support, &scope_sets);
    emit_scope_subroutines(emitter, data, &scope_sets);
    unset::emit_property_unset_helper(module, emitter, data, &slots, &scope_sets);
    emit_property_hash_slot_helper(module, emitter);
    emit_dynamic_property_exists_helper(module, emitter);
    emit_hydrate_declared_property_helper(module, emitter);
}

/// Emits `__elephc_eval_value_hydrate_property(Mixed* object, key, len, Mixed* value)`.
///
/// The interpreter's `unserialize()` hydrates an AOT object from the payload it parsed. PHP
/// writes each serialized key onto the DECLARED property it names, mangled (`"\0Class\0name"`,
/// `"\0*\0name"`) or not, ignoring visibility. The interpreter's ordinary `property_set` cannot
/// do that: the active scope is the caller's, so a protected slot is refused and the value lands
/// as a dynamic property beside a still-uninitialized typed one (Symfony's
/// `ContainerBuilder::$parameterBag`). This forwards to the same declared-slot matcher the
/// compiled decoder uses, which matches the mangled key first, then its unmangled tail.
fn emit_hydrate_declared_property_helper(module: &Module, emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- eval bridge: unserialize property hydration ---");
    label_c_global(
        module,
        emitter,
        "__elephc_eval_value_hydrate_property",
    );
    // The first argument is a boxed Mixed cell; `__rt_obj_store_prop` reads the class id from a
    // RAW object header, so unbox the payload before forwarding. The matcher allocates and
    // converts values on some property kinds, so keep a real frame around the call.
    match module.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                    // preserve the interpreter's frame and return address
            emitter.instruction("mov x29, sp");                                 // establish the shim frame
            emitter.instruction("ldr x0, [x0, #8]");                            // unbox the Mixed object cell to its payload
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_obj_store_prop");
            emitter.instruction("ldp x29, x30, [sp], #16");                      // restore the interpreter's frame and return address
            emitter.instruction("ret");                                         // return to the interpreter
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the interpreter's frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the shim frame
            emitter.instruction("sub rsp, 8");                                  // keep the call stack 16-byte aligned
            emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                // unbox the Mixed object cell to its payload
            crate::codegen_support::abi::emit_call_label(emitter, "__rt_obj_store_prop");
            emitter.instruction("add rsp, 8");                                  // release the alignment slot
            emitter.instruction("pop rbp");                                     // restore the interpreter's frame pointer
            emitter.instruction("ret");                                         // return to the interpreter
        }
    }
}

/// Numbers the distinct visibility scope lists, so each one gets exactly one emitted check.
///
/// WHY. The bridge used to test the active eval scope against every allowed class name INLINE at
/// each slot, and a class's slots all share one list. Measured on the Symfony `--web` module,
/// that was **522 123 `__rt_strcasecmp` call sites of the 608 413 in the whole program** -- about
/// seven lines each, and the dominant cost of the three property helpers. The comparisons are
/// unchanged and run in the same order; they now live in one subroutine per list, which each
/// site reaches in five instructions whatever the length of the list.
///
/// The numbering is by the sorted list, so it is a function of the module rather than of the
/// order the slots happen to be visited in.
fn index_scope_sets(slots: &[EvalPropertySlot]) -> BTreeMap<Vec<String>, usize> {
    let mut distinct: Vec<Vec<String>> = slots
        .iter()
        .filter(|slot| !matches!(slot.visibility, Visibility::Public))
        .map(|slot| slot.allowed_scopes.clone())
        .collect();
    distinct.sort();
    distinct.dedup();
    distinct
        .into_iter()
        .enumerate()
        .map(|(index, names)| (names, index))
        .collect()
}

/// Returns the label of the subroutine answering "is the active eval scope in this list".
fn scope_subroutine_label(index: usize, arch: Arch) -> String {
    match arch {
        Arch::AArch64 => format!("__elephc_eval_scope_ok_{}", index),
        Arch::X86_64 => format!("__elephc_eval_scope_ok_{}_x", index),
    }
}

/// Emits one subroutine per distinct scope list: scope pointer/length in, 1 or 0 out.
fn emit_scope_subroutines(
    emitter: &mut Emitter,
    data: &mut DataSection,
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    for (names, index) in scope_sets {
        match emitter.target.arch {
            Arch::AArch64 => emit_aarch64_scope_subroutine(emitter, data, names, *index),
            Arch::X86_64 => emit_x86_64_scope_subroutine(emitter, data, names, *index),
        }
    }
}

/// Emits the ARM64 scope subroutine: `x1` scope pointer, `x2` scope length, `x0` = 1 when allowed.
fn emit_aarch64_scope_subroutine(
    emitter: &mut Emitter,
    data: &mut DataSection,
    names: &[String],
    index: usize,
) {
    let label = scope_subroutine_label(index, Arch::AArch64);
    let allowed = format!("{}_allowed", label);
    let denied = format!("{}_denied", label);
    let ret = format!("{}_ret", label);
    emitter.blank();
    emitter.label(&label);
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // preserve the caller frame across the comparisons
    emitter.instruction("str x1, [sp, #16]");                                   // keep the active eval class-scope pointer
    emitter.instruction("str x2, [sp, #24]");                                   // keep the active eval class-scope length
    emitter.instruction(&format!("cbz x1, {}", denied));                        // reject scoped access outside a class scope
    for scope_name in names {
        let (string_label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction("ldr x1, [sp, #16]");                               // reload the active eval class-scope pointer
        emitter.instruction("ldr x2, [sp, #24]");                               // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "x3", &string_label);
        abi::emit_load_int_immediate(emitter, "x4", len as i64);
        emitter.instruction("bl __rt_strcasecmp");                              // compare current eval scope with an allowed class
        emitter.instruction(&format!("cbz x0, {}", allowed));                   // accept access when the current scope is allowed
    }
    emitter.label(&denied);
    emitter.instruction("mov x0, xzr");                                         // report that the active scope is not allowed
    emitter.instruction(&format!("b {}", ret));                                 // join the single exit
    emitter.label(&allowed);
    emitter.instruction("mov x0, #1");                                          // report that the active scope is allowed
    emitter.label(&ret);
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore the caller frame
    emitter.instruction("ret");                                                 // return the visibility answer
}

/// Emits the x86_64 twin: `rdi` scope pointer, `rsi` scope length, `rax` = 1 when allowed.
fn emit_x86_64_scope_subroutine(
    emitter: &mut Emitter,
    data: &mut DataSection,
    names: &[String],
    index: usize,
) {
    let label = scope_subroutine_label(index, Arch::X86_64);
    let allowed = format!("{}_allowed", label);
    let denied = format!("{}_denied", label);
    let ret = format!("{}_ret", label);
    emitter.blank();
    emitter.label(&label);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the subroutine frame
    emitter.instruction("sub rsp, 16");                                         // keep the nested call 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the active eval class-scope length
    emitter.instruction("test rdi, rdi");                                       // check whether eval is executing inside a class scope
    emitter.instruction(&format!("jz {}", denied));                             // reject scoped access outside a class scope
    for scope_name in names {
        let (string_label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // reload the active eval class-scope pointer
        emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                   // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "rdx", &string_label);
        abi::emit_load_int_immediate(emitter, "rcx", len as i64);
        emitter.instruction("call __rt_strcasecmp");                            // compare current eval scope with an allowed class
        emitter.instruction("test rax, rax");                                   // check whether the current scope matched
        emitter.instruction(&format!("je {}", allowed));                        // accept access when the current scope is allowed
    }
    emitter.label(&denied);
    emitter.instruction("xor eax, eax");                                        // report that the active scope is not allowed
    emitter.instruction(&format!("jmp {}", ret));                               // join the single exit
    emitter.label(&allowed);
    emitter.instruction("mov eax, 1");                                          // report that the active scope is allowed
    emitter.label(&ret);
    emitter.instruction("mov rsp, rbp");                                        // discard the subroutine frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the visibility answer
}

/// Collects declared properties with storage layouts and visibility rules the bridge can access.
fn collect_eval_property_slots(module: &Module) -> Vec<EvalPropertySlot> {
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_property_slots(module, class_name, class_info, &mut slots);
    }
    slots
}

/// Adds bridge-supported properties for one class.
fn collect_class_property_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    slots: &mut Vec<EvalPropertySlot>,
) {
    for (index, (property, ty)) in class_info.properties.iter().enumerate() {
        let visible_index = class_info.visible_property_index(property);
        let (declaring_class, visibility, is_hidden_shadow) = if visible_index == Some(index) {
            let declaring_class = class_info
                .property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name)
                .to_string();
            (
                declaring_class,
                property_visibility(class_info, property).clone(),
                false,
            )
        } else {
            let Some((declaring_class, visibility)) =
                hidden_private_property_slot_metadata(module, class_name, index, property)
            else {
                continue;
            };
            (declaring_class, visibility, true)
        };
        if !property_visibility_supported(&visibility) || !property_type_supported(ty) {
            continue;
        }
        slots.push(EvalPropertySlot {
            class_id: class_info.class_id,
            class_name: class_name.to_string(),
            declaring_class: declaring_class.clone(),
            allowed_scopes: visibility_scope_names(module, &declaring_class, &visibility),
            property: property.clone(),
            visibility,
            offset: 8 + index * 16,
            ty: super::eval_argument_helpers::bridge_storage_type(ty),
            is_declared: class_info.property_slot_is_declared(index, property),
            is_reference: class_info.property_slot_is_reference(index, property),
            is_hidden_shadow,
        });
    }
}

/// Returns metadata for a private parent slot hidden by a same-named child property.
fn hidden_private_property_slot_metadata(
    module: &Module,
    class_name: &str,
    index: usize,
    property: &str,
) -> Option<(String, Visibility)> {
    for (ancestor_name, ancestor_info) in class_ancestry(module, class_name) {
        let Some((ancestor_property, _)) = ancestor_info.properties.get(index) else {
            continue;
        };
        if ancestor_property != property
            || ancestor_info.visible_property_index(property) != Some(index)
        {
            continue;
        }
        let visibility = property_visibility(ancestor_info, property).clone();
        if visibility != Visibility::Private {
            continue;
        }
        let declaring_class = ancestor_info
            .property_declaring_classes
            .get(property)
            .cloned()
            .unwrap_or_else(|| ancestor_name.to_string());
        return Some((declaring_class, visibility));
    }
    None
}

/// Returns class metadata from root parent to the requested class.
fn class_ancestry<'a>(module: &'a Module, class_name: &'a str) -> Vec<(&'a str, &'a ClassInfo)> {
    let mut chain = Vec::new();
    collect_class_ancestry(module, class_name, &mut chain);
    chain
}

/// Recursively collects class metadata from parent to child.
fn collect_class_ancestry<'a>(
    module: &'a Module,
    class_name: &'a str,
    chain: &mut Vec<(&'a str, &'a ClassInfo)>,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    if let Some(parent) = class_info.parent.as_deref() {
        collect_class_ancestry(module, parent, chain);
    }
    chain.push((class_name, class_info));
}

/// Returns the declared property visibility, defaulting to public metadata.
fn property_visibility<'a>(class_info: &'a ClassInfo, property: &str) -> &'a Visibility {
    class_info
        .property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public)
}

/// Returns true when the eval property bridge can enforce this visibility.
fn property_visibility_supported(visibility: &Visibility) -> bool {
    matches!(
        visibility,
        Visibility::Public | Visibility::Protected | Visibility::Private
    )
}

/// Returns true for property storage shapes the bridge can box and update.
/// `Void` slots (untyped properties never assigned by AOT code) are read-only:
/// they box as PHP null and report initialized, while writes fail in the store
/// bodies because the slot has no value storage.
fn property_type_supported(ty: &PhpType) -> bool {
    crate::types::is_nullable_pointer_type(ty) || matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Object(_)
            | PhpType::Callable
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Void
    )
}

/// Emits `__elephc_eval_value_property_get(Mixed*, name, len, scope, scope_len) -> Mixed*`.
fn emit_property_get_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user property get ---");
    label_c_global(module, emitter, "__elephc_eval_value_property_get");
    match module.target.arch {
        Arch::AArch64 => emit_property_get_aarch64(module, emitter, data, slots, scope_sets),
        Arch::X86_64 => emit_property_get_x86_64(module, emitter, data, slots, scope_sets),
    }
}

/// Emits `__elephc_eval_value_property_is_initialized(Mixed*, name, len, scope, scope_len) -> bool`.
fn emit_property_is_initialized_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user property initialization probe ---");
    label_c_global(
        module,
        emitter,
        "__elephc_eval_value_property_is_initialized",
    );
    match module.target.arch {
        Arch::AArch64 => {
            emit_property_is_initialized_aarch64(module, emitter, data, slots, scope_sets)
        }
        Arch::X86_64 => {
            emit_property_is_initialized_x86_64(module, emitter, data, slots, scope_sets)
        }
    }
}

/// Emits `__elephc_eval_value_property_set(Mixed*, name, len, Mixed*, scope, scope_len) -> bool`.
fn emit_property_set_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    callable_support: &EvalCallableDescriptorSupport,
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user property set ---");
    label_c_global(module, emitter, "__elephc_eval_value_property_set");
    match module.target.arch {
        Arch::AArch64 => {
            emit_property_set_aarch64(module, emitter, data, slots, callable_support, scope_sets)
        }
        Arch::X86_64 => emit_property_set_x86_64(module, emitter, data, slots, callable_support, scope_sets),
    }
}

/// Emits `__elephc_eval_value_dynamic_property_exists(Mixed*, name, len) -> bool`.
///
/// This is intentionally distinct from the object-property iterator. That iterator is a public
/// projection used by JSON and reflection, while this probe answers only whether the generated
/// user-class property hash already owns one exact key before Magician considers `__set`.
fn emit_dynamic_property_exists_helper(module: &Module, emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- eval bridge: dynamic property existence probe ---");
    label_c_global(
        module,
        emitter,
        "__elephc_eval_value_dynamic_property_exists",
    );
    match module.target.arch {
        Arch::AArch64 => emit_dynamic_property_exists_aarch64(emitter),
        Arch::X86_64 => emit_dynamic_property_exists_x86_64(emitter),
    }
}

/// Emits the ARM64 dynamic-property existence probe.
fn emit_dynamic_property_exists_aarch64(emitter: &mut Emitter) {
    let miss = "__elephc_eval_value_dynamic_property_exists_miss";
    let done = "__elephc_eval_value_dynamic_property_exists_done";
    emitter.instruction("sub sp, sp, #48");                                     // reserve an aligned frame for the key and saved caller state
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #32");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the requested dynamic-property name
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the requested name length
    emitter.instruction(&format!("cbz x0, {miss}"));                            // a null boxed receiver owns no dynamic entry
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the receiver tag and native object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 identifies an object payload
    emitter.instruction(&format!("b.ne {miss}"));                               // non-object values own no dynamic entry
    emitter.instruction("mov x0, x1");                                          // pass the raw native object to the layout-specific slot helper
    abi::emit_call_label(emitter, dynamic_properties::SLOT_HELPER);
    emitter.instruction(&format!("cbz x0, {miss}"));                            // objects without permitted hash storage cannot contain the key
    emitter.instruction("ldr x0, [x0]");                                        // load the current per-instance property hash
    emitter.instruction(&format!("cbz x0, {miss}"));                            // an unallocated hash has no entries
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore the requested property name for lookup
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore the requested property-name length
    abi::emit_call_label(emitter, "__rt_hash_get");
    emitter.instruction("cmp x4, #0");                                          // use the entry-address result, including false and null values
    emitter.instruction("cset x0, ne");                                         // return true exactly when the hash contains the key
    emitter.instruction(&format!("b {done}"));                                  // join the common helper epilogue
    emitter.label(miss);
    emitter.instruction("mov x0, xzr");                                         // report that no public dynamic entry exists
    emitter.label(done);
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #48");                                     // release the temporary helper frame
    emitter.instruction("ret");                                                 // return the exact-key existence flag
}

/// Emits the x86_64 dynamic-property existence probe.
fn emit_dynamic_property_exists_x86_64(emitter: &mut Emitter) {
    let miss = "__elephc_eval_value_dynamic_property_exists_miss_x";
    let done = "__elephc_eval_value_dynamic_property_exists_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spills for the requested key
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the requested dynamic-property name
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the requested name length
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed receiver is null
    emitter.instruction(&format!("jz {miss}"));                                 // a null boxed receiver owns no dynamic entry
    emitter.instruction("mov rax, rdi");                                        // pass the boxed receiver through the mixed-unbox ABI
    emitter.instruction("call __rt_mixed_unbox");                               // expose the receiver tag and native object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 identifies an object payload
    emitter.instruction(&format!("jne {miss}"));                                // non-object values own no dynamic entry
    abi::emit_call_label(emitter, dynamic_properties::SLOT_HELPER);
    emitter.instruction("test rax, rax");                                       // inspect the layout-specific hash-slot address
    emitter.instruction(&format!("jz {miss}"));                                 // objects without permitted hash storage cannot contain the key
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // load the current per-instance property hash
    emitter.instruction("test rdi, rdi");                                       // check whether a hash was allocated
    emitter.instruction(&format!("jz {miss}"));                                 // an unallocated hash has no entries
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // restore the requested property name for lookup
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the requested property-name length
    abi::emit_call_label(emitter, "__rt_hash_get");
    emitter.instruction("test r8, r8");                                         // use the entry-address result, including false and null values
    emitter.instruction("setne al");                                            // materialize true when the hash contains the key
    emitter.instruction("movzx rax, al");                                       // widen the boolean return value to the C ABI word
    emitter.instruction(&format!("jmp {done}"));                                // join the common helper epilogue
    emitter.label(miss);
    emitter.instruction("xor eax, eax");                                        // report that no public dynamic entry exists
    emitter.label(done);
    emitter.instruction("leave");                                               // release spills and restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the exact-key existence flag
}

/// Emits the ARM64 property-get helper body.
fn emit_property_get_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    let null_label = "__elephc_eval_value_property_get_null";
    let fail_label = "__elephc_eval_value_property_get_fail";
    let done_label = "__elephc_eval_value_property_get_done";
    emitter.instruction("sub sp, sp, #80");                                     // reserve helper frame for saved inputs, object, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested property-name length
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed receiver for stdClass fallback reads
    emitter.instruction("str x3, [sp, #32]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x4, [sp, #40]");                                   // save the active eval class-scope length
    emitter.instruction(&format!("cbz x0, {}", null_label));                    // null Mixed receiver reads as PHP null
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", null_label));                       // non-object receivers read as PHP null
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for property loads
    emitter.instruction("ldr x9, [x1]");                                        // load the object's runtime class id
    emit_aarch64_property_dispatch_scoped(module, emitter, data, slots, scope_sets, "get", fail_label);
    emit_dynamic_property_get_fallback(emitter, null_label);
    emitter.instruction(&format!("b {}", done_label));                          // return after stdClass fallback get or null result
    emit_aarch64_get_slot_bodies(module, emitter, data, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // report an inaccessible declared property read to Rust
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after access failure
    emitter.label(null_label);
    let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(emitter, &null_symbol);
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed property value to Rust
}

/// Emits the x86_64 property-get helper body.
fn emit_property_get_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    let null_label = "__elephc_eval_value_property_get_null_x";
    let fail_label = "__elephc_eval_value_property_get_fail_x";
    let done_label = "__elephc_eval_value_property_get_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for name, length, object, and scope
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the boxed receiver for stdClass fallback reads
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the active eval class-scope length
    emitter.instruction(&format!("test rdi, rdi"));                             // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", null_label));                         // null Mixed receiver reads as PHP null
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", null_label));                        // non-object receivers read as PHP null
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for property loads
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the object's runtime class id
    emit_x86_64_property_dispatch_scoped(module, emitter, data, slots, scope_sets, "get", fail_label);
    emit_dynamic_property_get_fallback(emitter, null_label);
    emitter.instruction(&format!("jmp {}", done_label));                        // return after stdClass fallback get or null result
    emit_x86_64_get_slot_bodies(module, emitter, data, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // report an inaccessible declared property read to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after access failure
    emitter.label(null_label);
    let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(emitter, &null_symbol);
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed property value to Rust
}

/// Emits the ARM64 property-initialization helper body.
fn emit_property_is_initialized_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    let fail_label = "__elephc_eval_value_property_is_initialized_fail";
    let done_label = "__elephc_eval_value_property_is_initialized_done";
    emitter.instruction("sub sp, sp, #80");                                     // reserve helper frame for saved inputs, object, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested property-name length
    emitter.instruction("str x3, [sp, #32]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x4, [sp, #40]");                                   // save the active eval class-scope length
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // null Mixed receiver cannot have an initialized declared property
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", fail_label));                       // non-object receivers cannot have initialized declared properties
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for marker loads
    emitter.instruction("ldr x9, [x1]");                                        // load the object's runtime class id
    emit_aarch64_property_dispatch_scoped(module, emitter, data, slots, scope_sets, "is_initialized", fail_label);
    emitter.instruction(&format!("b {}", fail_label));                          // no supported declared property matched the request
    emit_aarch64_initialized_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0");                                          // report an initialization miss to Rust
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after a miss
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the initialization flag to Rust
}

/// Emits the x86_64 property-initialization helper body.
fn emit_property_is_initialized_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    let fail_label = "__elephc_eval_value_property_is_initialized_fail_x";
    let done_label = "__elephc_eval_value_property_is_initialized_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for name, object, and scope
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the active eval class-scope length
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", fail_label));                         // null Mixed receiver cannot have an initialized declared property
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", fail_label));                        // non-object receivers cannot have initialized declared properties
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for marker loads
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the object's runtime class id
    emit_x86_64_property_dispatch_scoped(module, emitter, data, slots, scope_sets, "is_initialized", fail_label);
    emitter.instruction(&format!("jmp {}", fail_label));                        // no supported declared property matched the request
    emit_x86_64_initialized_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // report an initialization miss to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after a miss
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the initialization flag to Rust
}

/// Emits the ARM64 property-set helper body.
fn emit_property_set_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    callable_support: &EvalCallableDescriptorSupport,
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    let fail_label = "__elephc_eval_value_property_set_fail";
    let done_label = "__elephc_eval_value_property_set_done";
    emitter.instruction("sub sp, sp, #96");                                     // reserve helper frame for inputs, object, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested property-name length
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed value being assigned
    emitter.instruction("str x0, [sp, #32]");                                   // save the boxed receiver for stdClass fallback writes
    emitter.instruction("str x4, [sp, #40]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x5, [sp, #48]");                                   // save the active eval class-scope length
    emitter.instruction("str x6, [sp, #56]");                                   // save the active eval context for callable conversion
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // null Mixed receiver cannot accept a property write
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", fail_label));                       // non-object receivers reject the property write
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for property stores
    emitter.instruction("ldr x9, [x1]");                                        // load the object's runtime class id
    emit_aarch64_property_dispatch_scoped(module, emitter, data, slots, scope_sets, "set", fail_label);
    emit_dynamic_property_set_fallback(emitter, fail_label, done_label);
    emit_aarch64_set_slot_bodies(
        module,
        emitter,
        data,
        slots,
        done_label,
        fail_label,
        callable_support,
    );
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0");                                          // report a failed eval property write to Rust
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the write-status flag to Rust
}

/// Emits the x86_64 property-set helper body.
fn emit_property_set_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    callable_support: &EvalCallableDescriptorSupport,
    scope_sets: &BTreeMap<Vec<String>, usize>,
) {
    let fail_label = "__elephc_eval_value_property_set_fail_x";
    let done_label = "__elephc_eval_value_property_set_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve aligned slots for name, length, object, value, scope, and context
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed value being assigned
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the boxed receiver for stdClass fallback writes
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // save the active eval class-scope length
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // the seventh integer argument arrives on the stack
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the active eval context for callable conversion
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", fail_label));                         // null Mixed receiver cannot accept a property write
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", fail_label));                        // non-object receivers reject the property write
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for property stores
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the object's runtime class id
    emit_x86_64_property_dispatch_scoped(module, emitter, data, slots, scope_sets, "set", fail_label);
    emit_dynamic_property_set_fallback(emitter, fail_label, done_label);
    emit_x86_64_set_slot_bodies(
        module,
        emitter,
        data,
        slots,
        done_label,
        fail_label,
        callable_support,
    );
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // report a failed eval property write to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the write-status flag to Rust
}


/// Emits ARM64 class-id and property-name dispatch for helper slot bodies.
fn emit_aarch64_property_dispatch_scoped(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
    mode: &str,
    fail_label: &str,
) {
    let groups = grouped_slots(slots).into_iter().collect::<Vec<_>>();
    let ids = groups.iter().map(|(class_id, _)| *class_id).collect::<Vec<_>>();
    let load_id = |emitter: &mut Emitter, reg: &str| {
        emitter.instruction(&format!("ldr {}, [sp, #16]", reg));                // reload the unboxed object pointer before this class test
        emitter.instruction(&format!("ldr {0}, [{0}]", reg));                   // load the object's runtime class id
    };
    let stem = format!("__elephc_eval_property_{}", mode);
    let switch = ClassIdSwitch { load_id: &load_id, stem: &stem, suffix: "" };
    emit_class_id_switch(emitter, &switch, &ids, &mut |emitter, index| {
        let (class_id, class_slots) = &groups[index];
        for slot in class_slots {
            emit_aarch64_property_name_compare(
                module, emitter, data, slot, scope_sets, mode, fail_label,
            );
        }
        // A name miss falls through to the next case test, which cannot match this id again.
        emitter.label(&format!("__elephc_eval_property_{}_class_{}", mode, class_id));
    });
}

/// Emits x86_64 class-id and property-name dispatch for helper slot bodies.
fn emit_x86_64_property_dispatch_scoped(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    scope_sets: &BTreeMap<Vec<String>, usize>,
    mode: &str,
    fail_label: &str,
) {
    let groups = grouped_slots(slots).into_iter().collect::<Vec<_>>();
    let ids = groups.iter().map(|(class_id, _)| *class_id).collect::<Vec<_>>();
    let load_id = |emitter: &mut Emitter, reg: &str| {
        emitter.instruction(&format!("mov {}, QWORD PTR [rbp - 24]", reg));     // reload the unboxed object pointer before this class test
        emitter.instruction(&format!("mov {0}, QWORD PTR [{0}]", reg));         // load the object's runtime class id
    };
    let stem = format!("__elephc_eval_property_{}", mode);
    let switch = ClassIdSwitch { load_id: &load_id, stem: &stem, suffix: "_x" };
    emit_class_id_switch(emitter, &switch, &ids, &mut |emitter, index| {
        let (class_id, class_slots) = &groups[index];
        for slot in class_slots {
            emit_x86_64_property_name_compare(
                module, emitter, data, slot, scope_sets, mode, fail_label,
            );
        }
        // A name miss falls through to the next case test, which cannot match this id again.
        emitter.label(&format!("__elephc_eval_property_{}_class_{}_x", mode, class_id));
    });
}

/// Emits one ARM64 property-name comparison and branch to the matching body.
fn emit_aarch64_property_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    scope_sets: &BTreeMap<Vec<String>, usize>,
    mode: &str,
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested property-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_str_eq");                                      // compare requested property name with this declared property
    let target_label = slot_body_label(module, slot, mode);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("cbnz x0, {}", target_label));             // dispatch to the property body when the names match
        return;
    }
    let miss_label = slot_access_miss_label(module, slot, mode);
    emitter.instruction(&format!("cbz x0, {}", miss_label));                    // continue property dispatch when names differ
    let scope_ok_label = slot_scope_ok_label(module, slot, mode);
    let private_shadow_label = format!("{scope_ok_label}_eval_shadow");
    let scope_fail_label = if slot.scope_miss_is_dynamic() {
        miss_label.as_str()
    } else if slot.visibility == Visibility::Private {
        private_shadow_label.as_str()
    } else {
        fail_label
    };
    emit_aarch64_property_scope_check(
        emitter, slot, scope_sets, mode, &scope_ok_label, scope_fail_label,
    );
    emitter.label(&scope_ok_label);
    emitter.instruction(&format!("b {}", target_label));                        // dispatch after scoped visibility is satisfied
    if slot.visibility == Visibility::Private && !slot.scope_miss_is_dynamic() {
        emitter.label(&private_shadow_label);
        private_shadow::emit_separate_property_probe(module, emitter, &miss_label, fail_label);
    }
    emitter.label(&miss_label);
}

/// Emits one x86_64 property-name comparison and branch to the matching body.
fn emit_x86_64_property_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    scope_sets: &BTreeMap<Vec<String>, usize>,
    mode: &str,
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested property-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested property-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_str_eq");                                    // compare requested property name with this declared property
    emitter.instruction("test rax, rax");                                       // check whether the property names matched
    let target_label = slot_body_label(module, slot, mode);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("jne {}", target_label));                  // dispatch to the property body when the names match
        return;
    }
    let miss_label = slot_access_miss_label(module, slot, mode);
    emitter.instruction(&format!("je {}", miss_label));                         // continue property dispatch when names differ
    let scope_ok_label = slot_scope_ok_label(module, slot, mode);
    let private_shadow_label = format!("{scope_ok_label}_eval_shadow");
    let scope_fail_label = if slot.scope_miss_is_dynamic() {
        miss_label.as_str()
    } else if slot.visibility == Visibility::Private {
        private_shadow_label.as_str()
    } else {
        fail_label
    };
    emit_x86_64_property_scope_check(
        emitter, slot, scope_sets, mode, &scope_ok_label, scope_fail_label,
    );
    emitter.label(&scope_ok_label);
    emitter.instruction(&format!("jmp {}", target_label));                      // dispatch after scoped visibility is satisfied
    if slot.visibility == Visibility::Private && !slot.scope_miss_is_dynamic() {
        emitter.label(&private_shadow_label);
        private_shadow::emit_separate_property_probe(module, emitter, &miss_label, fail_label);
    }
    emitter.label(&miss_label);
}

/// Emits the ARM64 visibility check for a protected/private property bridge hit.
///
/// The comparisons themselves live in one subroutine per distinct scope list; see
/// `index_scope_sets` for what that is worth.
fn emit_aarch64_property_scope_check(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    scope_sets: &BTreeMap<Vec<String>, usize>,
    mode: &str,
    success_label: &str,
    fail_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = aarch64_scope_offsets(mode);
    let index = scope_sets
        .get(&slot.allowed_scopes)
        .copied()
        .expect("every non-public slot's scope list is indexed");
    emitter.instruction(&format!("ldr x1, [sp, #{}]", scope_ptr_offset));       // pass the active eval class-scope pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", scope_len_offset));       // pass the active eval class-scope length
    abi::emit_call_label(emitter, &scope_subroutine_label(index, Arch::AArch64));
    emitter.instruction(&format!("cbnz x0, {}", success_label));                // accept access when the current scope is allowed
    emitter.instruction(&format!("b {}", fail_label));                          // reject scoped property access from unrelated classes
}

/// Emits the x86_64 visibility check for a protected/private property bridge hit.
fn emit_x86_64_property_scope_check(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    scope_sets: &BTreeMap<Vec<String>, usize>,
    mode: &str,
    success_label: &str,
    fail_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = x86_64_scope_offsets(mode);
    let index = scope_sets
        .get(&slot.allowed_scopes)
        .copied()
        .expect("every non-public slot's scope list is indexed");
    emitter.instruction(
        &format!("mov rdi, QWORD PTR [rbp - {}]", scope_ptr_offset)
    );
    emitter.instruction(                                                        // reload the active eval class-scope length
        &format!("mov rsi, QWORD PTR [rbp - {}]", scope_len_offset)
    );                                                                          // reload the active eval class-scope length
    abi::emit_call_label(emitter, &scope_subroutine_label(index, Arch::X86_64));
    emitter.instruction("test rax, rax");                                       // check whether the active scope is allowed
    emitter.instruction(&format!("jne {}", success_label));                     // accept access when the current scope is allowed
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject scoped property access from unrelated classes
}

/// Returns ARM64 stack offsets for the class-scope pointer and length.
fn aarch64_scope_offsets(mode: &str) -> (usize, usize) {
    match mode {
        "get" | "is_initialized" | "unset" => (32, 40),
        "set" => (40, 48),
        _ => unreachable!("unknown eval property bridge operation"),
    }
}

/// Returns x86_64 frame offsets for the class-scope pointer and length.
fn x86_64_scope_offsets(mode: &str) -> (usize, usize) {
    match mode {
        "get" | "is_initialized" | "unset" => (40, 48),
        "set" => (48, 56),
        _ => unreachable!("unknown eval property bridge operation"),
    }
}

/// Emits ARM64 property-get bodies for every bridge-supported property slot.
fn emit_aarch64_get_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "get"));
        if slot.is_reference {
            emit_aarch64_enter_reference_cell(emitter, slot, false);
        }
        let view = slot.cell_view();
        emit_aarch64_uninitialized_property_get_guard(module, emitter, data, &view, done_label);
        emit_aarch64_box_property_slot(emitter, &view);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the declared property value
    }
}

/// Emits x86_64 property-get bodies for every bridge-supported property slot.
fn emit_x86_64_get_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "get"));
        if slot.is_reference {
            emit_x86_64_enter_reference_cell(emitter, slot, false);
        }
        let view = slot.cell_view();
        emit_x86_64_uninitialized_property_get_guard(module, emitter, data, &view, done_label);
        emit_x86_64_box_property_slot(emitter, &view);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the declared property value
    }
}

/// Emits ARM64 property-initialization bodies for every bridge-supported property slot.
fn emit_aarch64_initialized_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "is_initialized"));
        if slot.is_reference {
            emit_aarch64_enter_reference_cell(emitter, slot, false);
        }
        emit_aarch64_property_initialized_flag(emitter, &slot.cell_view());
        emitter.instruction(&format!("b {}", done_label));                      // return after materializing the initialization flag
    }
}

/// Emits x86_64 property-initialization bodies for every bridge-supported property slot.
fn emit_x86_64_initialized_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "is_initialized"));
        if slot.is_reference {
            emit_x86_64_enter_reference_cell(emitter, slot, false);
        }
        emit_x86_64_property_initialized_flag(emitter, &slot.cell_view());
        emitter.instruction(&format!("jmp {}", done_label));                    // return after materializing the initialization flag
    }
}

/// Emits ARM64 property-set bodies for every bridge-supported property slot.
fn emit_aarch64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    done_label: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        if slot.is_reference {
            emit_aarch64_enter_reference_cell(emitter, slot, true);
        }
        emit_aarch64_store_property_slot(
            module,
            emitter,
            data,
            &slot.cell_view(),
            fail_label,
            callable_support,
        );
        emitter.instruction("mov x0, #1");                                      // report a successful eval property write to Rust
        emitter.instruction(&format!("b {}", done_label));                      // return after storing the declared property value
    }
}

/// Emits x86_64 property-set bodies for every bridge-supported property slot.
fn emit_x86_64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    done_label: &str,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        if slot.is_reference {
            emit_x86_64_enter_reference_cell(emitter, slot, true);
        }
        emit_x86_64_store_property_slot(
            module,
            emitter,
            data,
            &slot.cell_view(),
            fail_label,
            callable_support,
        );
        emitter.instruction("mov rax, 1");                                      // report a successful eval property write to Rust
        emitter.instruction(&format!("jmp {}", done_label));                    // return after storing the declared property value
    }
}

/// Replaces the object pointer in the helper frame with the slot's managed reference cell.
///
/// `clear_uninitialized` also resets a typed-uninitialized marker (the cell's value word) to zero,
/// so a write that replaces the old value never releases the sentinel as if it were a pointer.
fn emit_aarch64_enter_reference_cell(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    clear_uninitialized: bool,
) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer
    emitter.instruction(&format!("ldr x9, [x9, #{}]", slot.offset));            // load the managed reference-cell pointer from the property slot
    emitter.instruction("str x9, [sp, #16]");                                   // address the cell instead of the object from here on
    if clear_uninitialized {
        emitter.instruction("ldr x10, [x9]");                                   // load the cell value word
        abi::emit_load_int_immediate(emitter, "x11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        emitter.instruction("cmp x10, x11");                                    // is the owned reference still typed-uninitialized?
        emitter.instruction("csel x10, xzr, x10, eq");                          // drop the marker so the old value is never released
        emitter.instruction("str x10, [x9]");                                   // publish the cleared value word
    }
}

/// Replaces the object pointer in the helper frame with the slot's managed reference cell.
fn emit_x86_64_enter_reference_cell(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    clear_uninitialized: bool,
) {
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", slot.offset)); // load the managed reference-cell pointer from the property slot
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // address the cell instead of the object from here on
    if clear_uninitialized {
        emitter.instruction("mov rax, QWORD PTR [r11]");                        // load the cell value word
        abi::emit_load_int_immediate(emitter, "r10", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
        emitter.instruction("xor edx, edx");                                    // zero replaces the typed-uninitialized marker
        emitter.instruction("cmp rax, r10");                                    // is the owned reference still typed-uninitialized?
        emitter.instruction("cmove rax, rdx");                                  // drop the marker so the old value is never released
        emitter.instruction("mov QWORD PTR [r11], rax");                        // publish the cleared value word
    }
}

/// Emits an ARM64 boolean for one declared property's initialized state.
fn emit_aarch64_property_initialized_flag(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if !slot.is_declared && !slot_supports_untyped_unset_marker(slot) {
        emitter.instruction("mov x0, #1");                                      // marker-free untyped properties are always initialized
        return;
    }
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the unboxed object pointer
    emitter.instruction(&format!("ldr x11, [x10, #{}]", slot.marker_offset())); // load the typed-property initialization marker
    abi::emit_load_int_immediate(emitter, "x12", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x11, x12");                                        // compare the property marker against the uninitialized sentinel
    emitter.instruction("cset x0, ne");                                         // materialize true when the instance property is initialized
}

/// Emits an x86_64 boolean for one declared property's initialized state.
fn emit_x86_64_property_initialized_flag(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if !slot.is_declared && !slot_supports_untyped_unset_marker(slot) {
        emitter.instruction("mov rax, 1");                                      // marker-free untyped properties are always initialized
        return;
    }
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer
    emitter.instruction(                                                        // load the typed-property initialization marker
        &format!("mov rax, QWORD PTR [r11 + {}]", slot.marker_offset())
    );
    abi::emit_load_int_immediate(emitter, "r10", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp rax, r10");                                        // compare the property marker against the uninitialized sentinel
    emitter.instruction("setne al");                                            // materialize true when the instance property is initialized
    emitter.instruction("movzx rax, al");                                       // widen the initialization flag into the return register
}

/// Emits an ARM64 removed-state guard before boxing an eval bridge property read.
fn emit_aarch64_uninitialized_property_get_guard(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    done_label: &str,
) {
    if !slot.is_declared && !slot_supports_untyped_unset_marker(slot) {
        return;
    }
    let initialized_label = format!(
        "{}_initialized",
        slot_body_label_raw(slot, "get")
    );
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the unboxed object pointer for marker inspection
    emitter.instruction(&format!("ldr x11, [x10, #{}]", slot.marker_offset())); // load the typed-property initialization marker
    abi::emit_load_int_immediate(emitter, "x12", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x11, x12");                                        // compare the property marker against the uninitialized sentinel
    emitter.instruction(&format!("b.ne {}", initialized_label));                // continue boxing once the instance property is initialized
    if slot.is_declared {
        emitter.instruction("mov x0, xzr");                                     // report typed-uninitialized reads as bridge failures
    } else {
        emit_eval_untyped_removed_read(module, emitter, data, slot);
    }
    emitter.instruction(&format!("b {}", done_label));                          // return the typed failure or untyped boxed null
    emitter.label(&initialized_label);
}

/// Emits an x86_64 removed-state guard before boxing an eval bridge property read.
fn emit_x86_64_uninitialized_property_get_guard(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    done_label: &str,
) {
    if !slot.is_declared && !slot_supports_untyped_unset_marker(slot) {
        return;
    }
    let initialized_label = format!(
        "{}_initialized_x",
        slot_body_label_raw(slot, "get")
    );
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer for marker inspection
    emitter.instruction(                                                        // load the typed-property initialization marker
        &format!("mov rax, QWORD PTR [r10 + {}]", slot.marker_offset())
    );
    abi::emit_load_int_immediate(emitter, "r11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp rax, r11");                                        // compare the property marker against the uninitialized sentinel
    emitter.instruction(&format!("jne {}", initialized_label));                 // continue boxing once the instance property is initialized
    if slot.is_declared {
        emitter.instruction("xor eax, eax");                                    // report typed-uninitialized reads as bridge failures
    } else {
        emit_eval_untyped_removed_read(module, emitter, data, slot);
    }
    emitter.instruction(&format!("jmp {}", done_label));                        // return the typed failure or untyped boxed null
    emitter.label(&initialized_label);
}

/// Emits PHP's warning and returns an owned boxed null for one removed untyped slot.
fn emit_eval_untyped_removed_read(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
) {
    let message = format!(
        "Warning: Undefined property: {}::${}\n",
        slot.class_name.trim_start_matches('\\'),
        slot.property
    );
    let (label, len) = data.add_string(message.as_bytes());
    match module.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", &label);
            abi::emit_load_int_immediate(emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");
    let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(emitter, &null_symbol);
}

/// Boxes a property value loaded from an ARM64 object slot into a Mixed cell.
fn emit_aarch64_box_property_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer
    match slot.ty.codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset));    // load the array-or-hash property payload pointer
            emitter.instruction("bl __rt_mixed_from_array_kind");               // box with the payload's live indexed-or-hash shape, not the declared one
        }
        PhpType::Int | PhpType::Bool | PhpType::Object(_) => {
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset));    // load the property payload low word
            emitter.instruction("mov x2, xzr");                                 // heap/scalar property payloads do not use a high word here
            abi::emit_load_int_immediate(emitter, "x0", runtime_value_tag(&slot.ty) as i64);
            emitter.instruction("bl __rt_mixed_from_value");                    // box the property payload as a Mixed cell
        }
        // A `\Closure`-typed property holds a callable DESCRIPTOR. The tag has to come from the
        // REPRESENTATION, not from `slot.ty`: the declared type is `Object("Closure")`, whose
        // `runtime_value_tag` is 6, and boxing a descriptor as an object hands the interpreter a
        // pointer it would read as an instance.
        PhpType::Callable => {
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset));    // load the callable descriptor pointer
            emitter.instruction("mov x2, xzr");                                 // descriptor payloads do not use a high word
            emitter.instruction("mov x0, #10");                                 // runtime tag 10 = callable descriptor
            emitter.instruction("bl __rt_mixed_from_value");                    // box the descriptor as a Mixed cell
        }
        PhpType::Float => {
            emitter.instruction(&format!("ldr d0, [x9, #{}]", slot.offset));    // load the floating property payload
            emitter.instruction("fmov x1, d0");                                 // move float bits into the Mixed low payload word
            emitter.instruction("mov x2, xzr");                                 // float payloads do not use a high word
            emitter.instruction("mov x0, #2");                                  // runtime tag 2 = float
            emitter.instruction("bl __rt_mixed_from_value");                    // box the floating property payload as Mixed
        }
        PhpType::Str => {
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset));    // load the string property pointer
            emitter.instruction(&format!("ldr x2, [x9, #{}]", slot.offset + 8)); // load the string property length
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            emitter.instruction("bl __rt_mixed_from_value");                    // persist and box the string property payload
        }
        PhpType::Pointer(_) => {
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset));    // load the ?ptr property word
            emit_box_current_value_as_mixed(emitter, &slot.ty);
        }
        PhpType::TaggedScalar => {
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset));    // load the nullable integer property payload
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset + 8)); //load the nullable integer property tag
            emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let null_label = format!(
                "{}_mixed_null",
                slot_body_label_raw(slot, "get")
            );
            let done_label = format!(
                "{}_mixed_done",
                slot_body_label_raw(slot, "get")
            );
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset));    // load the stored Mixed property cell
            emitter.instruction(&format!("cbz x0, {}", null_label));            // null property storage reads as PHP null
            if slot.ty.is_php_array() {
                emitter.instruction("bl __rt_mixed_clone");                     // detach the array zval so eval writes preserve property COW aliases
            } else {
                emitter.instruction("bl __rt_incref");                          // retain the stored Mixed cell for the eval caller
            }
            emitter.instruction(&format!("b {}", done_label));                  // skip null materialization after a retained hit
            emitter.label(&null_label);
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
            emitter.label(&done_label);
        }
        _ => {
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
        }
    }
}

/// Boxes a property value loaded from an x86_64 object slot into a Mixed cell.
fn emit_x86_64_box_property_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer
    match slot.ty.codegen_repr() {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)); // load the array-or-hash property payload pointer
            emitter.instruction("call __rt_mixed_from_array_kind");             // box with the payload's live indexed-or-hash shape, not the declared one
        }
        PhpType::Int | PhpType::Bool | PhpType::Object(_) => {
            emitter.instruction(
                &format!("mov rdi, QWORD PTR [r11 + {}]", slot.offset)
            );
            emitter.instruction("xor esi, esi");                                // heap/scalar property payloads do not use a high word here
            abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(&slot.ty) as i64);
            emitter.instruction("call __rt_mixed_from_value");                  // box the property payload as a Mixed cell
        }
        // See the ARM64 twin: the tag is the REPRESENTATION's, not the declared type's.
        PhpType::Callable => {
            emitter.instruction(
                &format!("mov rdi, QWORD PTR [r11 + {}]", slot.offset)
            );                                                                  // load the callable descriptor pointer
            emitter.instruction("xor esi, esi");                                // descriptor payloads do not use a high word
            emitter.instruction("mov rax, 10");                                 // runtime tag 10 = callable descriptor
            emitter.instruction("call __rt_mixed_from_value");                  // box the descriptor as a Mixed cell
        }
        PhpType::Float => {
            emitter.instruction(                                                // load the floating property payload
                &format!("movsd xmm0, QWORD PTR [r11 + {}]", slot.offset)
            );
            emitter.instruction("movq rdi, xmm0");                              // move float bits into the Mixed low payload word
            emitter.instruction("xor esi, esi");                                // float payloads do not use a high word
            emitter.instruction("mov eax, 2");                                  // runtime tag 2 = float
            emitter.instruction("call __rt_mixed_from_value");                  // box the floating property payload as Mixed
        }
        PhpType::Str => {
            emitter.instruction(                                                // load the string property pointer
                &format!("mov rdi, QWORD PTR [r11 + {}]", slot.offset)
            );
            emitter.instruction(                                                // load the string property length
                &format!("mov rsi, QWORD PTR [r11 + {}]", slot.offset + 8)
            );
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string
            emitter.instruction("call __rt_mixed_from_value");                  // persist and box the string property payload
        }
        PhpType::Pointer(_) => {
            emitter.instruction(                                                // load the ?ptr property word
                &format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)
            );
            emit_box_current_value_as_mixed(emitter, &slot.ty);
        }
        PhpType::TaggedScalar => {
            emitter.instruction(                                                // load the nullable integer property payload
                &format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)
            );
            emitter.instruction(                                                // load the nullable integer property tag
                &format!("mov rdx, QWORD PTR [r11 + {}]", slot.offset + 8)
            );
            emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let null_label = format!("{}_mixed_null_x", slot_body_label_raw(slot, "get"));
            let done_label = format!("{}_mixed_done_x", slot_body_label_raw(slot, "get"));
            emitter.instruction(                                                // load the stored Mixed property cell
                &format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)
            );
            emitter.instruction("test rax, rax");                               // check whether the property storage is initialized
            emitter.instruction(&format!("jz {}", null_label));                 // null property storage reads as PHP null
            if slot.ty.is_php_array() {
                emitter.instruction("call __rt_mixed_clone");                   // detach the array zval so eval writes preserve property COW aliases
            } else {
                emitter.instruction("call __rt_incref");                        // retain the stored Mixed cell for the eval caller
            }
            emitter.instruction(&format!("jmp {}", done_label));                // skip null materialization after a retained hit
            emitter.label(&null_label);
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
            emitter.label(&done_label);
        }
        _ => {
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
        }
    }
}

/// Stores a boxed Mixed eval value into an ARM64 object property slot.
fn emit_aarch64_store_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if slot.ty.is_php_array() {
        emitter.instruction("ldr x0, [sp, #24]");                               // borrow the boxed array assignment before checking its PHP type
        super::eval_argument_helpers::emit_require_php_array(emitter, fail_label);
    }
    match slot.ty.codegen_repr() {
        PhpType::Int | PhpType::Pointer(_) => {
            emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "x0")
        }
        PhpType::Bool => {
            emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "x0")
        }
        PhpType::Float => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value for float coercion
            emitter.instruction("bl __rt_mixed_cast_float");                    // coerce the eval value to a PHP float
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str d0, [x9, #{}]", slot.offset));    // store the coerced float into the property slot
            emit_aarch64_clear_scalar_property_marker(emitter, slot);
        }
        PhpType::Str => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value for string coercion
            emitter.instruction("bl __rt_mixed_cast_string");                   // coerce the eval value to a PHP string pair
            emit_owned_slot_replacement(emitter, slot);
        }
        PhpType::TaggedScalar => emit_aarch64_store_tagged_scalar_property(emitter, slot),
        PhpType::Array(_) => emit_aarch64_store_heap_property_slot(emitter, slot, 4, fail_label),
        PhpType::AssocArray { .. } => {
            emit_aarch64_store_heap_property_slot(emitter, slot, 5, fail_label);
        }
        // The interpreter hands over a BOXED callback; the slot holds a DESCRIPTOR. This is the
        // same conversion a `callable` parameter gets, pointed at this helper's own frame.
        PhpType::Callable => {
            emit_aarch64_cast_eval_callable_arg(
                module,
                emitter,
                data,
                callable_support,
                &slot_body_label_raw(slot, "set"),
                fail_label,
                AARCH64_SET_CONTEXT_FROM_FP_OFFSET,
                AARCH64_SET_VALUE_FROM_FP_OFFSET,
            );
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str x0, [x9, #{}]", slot.offset));    // publish the converted descriptor
            emitter.instruction(&format!("str xzr, [x9, #{}]", slot.offset + 8)); // clear the typed-property initialization marker
        }
        PhpType::Object(class_name) => {
            emit_aarch64_store_object_property_slot(
                module,
                emitter,
                data,
                slot,
                &class_name,
                fail_label,
            );
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value being assigned
            emitter.instruction("bl __rt_incref");                              // retain the Mixed cell for property ownership
            emit_owned_slot_replacement(emitter, slot);
        }
        PhpType::Void => {
            emitter.instruction(&format!("b {}", fail_label));                  // Void slots have no value storage; report the eval write as unsupported
        }
        _ => {}
    }
}

/// Stores a boxed Mixed eval value into an x86_64 object property slot.
fn emit_x86_64_store_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    fail_label: &str,
    callable_support: &EvalCallableDescriptorSupport,
) {
    if slot.ty.is_php_array() {
        emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                   // borrow the boxed array assignment before checking its PHP type
        super::eval_argument_helpers::emit_require_php_array(emitter, fail_label);
    }
    match slot.ty.codegen_repr() {
        PhpType::Int | PhpType::Pointer(_) => {
            emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "rax")
        }
        PhpType::Bool => {
            emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "rax")
        }
        PhpType::Float => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value for float coercion
            emitter.instruction("call __rt_mixed_cast_float");                  // coerce the eval value to a PHP float
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(                                                // store the coerced float into the property slot
                &format!("movsd QWORD PTR [r11 + {}], xmm0", slot.offset)
            );
            emit_x86_64_clear_scalar_property_marker(emitter, slot);
        }
        PhpType::Str => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value for string coercion
            emitter.instruction("call __rt_mixed_cast_string");                 // coerce the eval value to a PHP string pair
            emit_owned_slot_replacement(emitter, slot);
        }
        PhpType::TaggedScalar => emit_x86_64_store_tagged_scalar_property(emitter, slot),
        PhpType::Array(_) => emit_x86_64_store_heap_property_slot(emitter, slot, 4, fail_label),
        PhpType::AssocArray { .. } => {
            emit_x86_64_store_heap_property_slot(emitter, slot, 5, fail_label);
        }
        // See the ARM64 twin.
        PhpType::Callable => {
            emit_x86_64_cast_eval_callable_arg(
                module,
                emitter,
                data,
                callable_support,
                &slot_body_label_raw(slot, "set"),
                fail_label,
                X86_64_SET_CONTEXT_FRAME_OFFSET,
                X86_64_SET_VALUE_FRAME_OFFSET,
            );
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(
                &format!("mov QWORD PTR [r11 + {}], rax", slot.offset)
            );                                                                  // publish the converted descriptor
            emitter.instruction(
                &format!("mov QWORD PTR [r11 + {}], 0", slot.offset + 8)
            );                                                                  // clear the typed-property initialization marker
        }
        PhpType::Object(class_name) => {
            emit_x86_64_store_object_property_slot(
                module,
                emitter,
                data,
                slot,
                &class_name,
                fail_label,
            );
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value being assigned
            emitter.instruction("call __rt_incref");                            // retain the Mixed cell for property ownership
            emit_owned_slot_replacement(emitter, slot);
        }
        PhpType::Void => {
            emitter.instruction(&format!("jmp {}", fail_label));                // Void slots have no value storage; report the eval write as unsupported
        }
        _ => {}
    }
}

/// Emits an ARM64 scalar property store after Mixed coercion.
fn emit_aarch64_store_cast_scalar(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    helper: &str,
    result_reg: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed eval value for scalar coercion
    emitter.instruction(&format!("bl {}", helper));                             // coerce the eval value to the declared property type
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer for the store
    emitter.instruction(&format!("str {}, [x9, #{}]", result_reg, slot.offset)); // store the coerced scalar into the property slot
    emit_aarch64_clear_scalar_property_marker(emitter, slot);
}

/// Clears an ARM64 one-word scalar typed-property marker after a successful store.
fn emit_aarch64_clear_scalar_property_marker(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if slot.is_declared {
        emitter.instruction(&format!("str xzr, [x9, #{}]", slot.offset + 8));   // clear the typed-property initialization marker
    }
}

/// Stores a boxed eval value into an ARM64 nullable-int tagged-scalar property slot.
fn emit_aarch64_store_tagged_scalar_property(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    let null_label = format!(
        "{}_tagged_scalar_null",
        slot_body_label_raw(slot, "set")
    );
    let done_label = format!(
        "{}_tagged_scalar_done",
        slot_body_label_raw(slot, "set")
    );
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed eval value for nullable-int inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the assigned value tag and payload words
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("b.eq {}", null_label));                       // materialize a tagged null for null property writes
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed eval value for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("b {}", done_label));                          // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer for the store
    emitter.instruction(&format!("str x0, [x9, #{}]", slot.offset));            // store the nullable integer payload into the property slot
    emitter.instruction(&format!("str x1, [x9, #{}]", slot.offset + 8));        // store the nullable integer tag into the property slot
}

/// Emits an x86_64 scalar property store after Mixed coercion.
fn emit_x86_64_store_cast_scalar(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    helper: &str,
    result_reg: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed eval value for scalar coercion
    emitter.instruction(&format!("call {}", helper));                           // coerce the eval value to the declared property type
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer for the store
    emitter.instruction(                                                        // store the coerced scalar into the property slot
        &format!("mov QWORD PTR [r11 + {}], {}", slot.offset, result_reg)
    );
    emit_x86_64_clear_scalar_property_marker(emitter, slot);
}

/// Clears an x86_64 one-word scalar typed-property marker after a successful store.
fn emit_x86_64_clear_scalar_property_marker(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if slot.is_declared {
        emitter.instruction(                                                    // clear the typed-property initialization marker
            &format!("mov QWORD PTR [r11 + {}], 0", slot.offset + 8)
        );
    }
}

/// Stores a boxed ARM64 eval heap value into an array-like property slot.
fn emit_aarch64_store_heap_property_slot(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed eval value for heap payload inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the assigned heap value tag and payload pointer
    if expected_tag == 4 {
        let shape_ok_label = format!("{}_array_shape_ok", slot_body_label_raw(slot, "set"));
        emitter.instruction("cmp x0, #4");                                      // accept indexed storage for an array-compatible property
        emitter.instruction(&format!("b.eq {}", shape_ok_label));               // keep the indexed payload without conversion
        emitter.instruction("cmp x0, #5");                                      // associative storage is also valid for a general array slot
        emitter.instruction(&format!("b.ne {}", fail_label));                   // reject non-array heap values
        emitter.label(&shape_ok_label);
    } else {
        abi::emit_load_int_immediate(emitter, "x10", expected_tag);
        emitter.instruction("cmp x0, x10");                                     // compare the assigned value tag with the property storage ABI
        emitter.instruction(&format!("b.ne {}", fail_label));                   // let the interpreter convert dense arrays required as hashes
    }
    emitter.instruction("mov x0, x1");                                          // move the unboxed heap pointer into the retained-result register
    abi::emit_incref_if_refcounted(emitter, &slot.ty.codegen_repr());
    emit_owned_slot_replacement(emitter, slot);
}

/// Validates and stores a boxed ARM64 eval object into an object property slot.
fn emit_aarch64_store_object_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("ldr x0, [sp, #24]");                               // reload the boxed eval value for object type validation
        abi::emit_symbol_address(emitter, "x1", &label);
        abi::emit_load_int_immediate(emitter, "x2", len as i64);
        emitter.instruction("mov x3, xzr");                                     // allow exact class matches for object property type hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emitter.instruction(&format!("cbz x0, {}", fail_label));                // reject values that fail the object property type hint
    }
    emit_aarch64_store_heap_property_slot(emitter, slot, 6, fail_label);
}

/// Stores a boxed x86_64 eval heap value into an array-like property slot.
fn emit_x86_64_store_heap_property_slot(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed eval value for heap payload inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the assigned heap value tag and payload pointer
    if expected_tag == 4 {
        let shape_ok_label = format!("{}_array_shape_ok", slot_body_label_raw(slot, "set"));
        emitter.instruction("cmp rax, 4");                                      // accept indexed storage for an array-compatible property
        emitter.instruction(&format!("je {}", shape_ok_label));                 // keep the indexed payload without conversion
        emitter.instruction("cmp rax, 5");                                      // associative storage is also valid for a general array slot
        emitter.instruction(&format!("jne {}", fail_label));                    // reject non-array heap values
        emitter.label(&shape_ok_label);
    } else {
        abi::emit_load_int_immediate(emitter, "r10", expected_tag);
        emitter.instruction("cmp rax, r10");                                    // compare the assigned value tag with the property storage ABI
        emitter.instruction(&format!("jne {}", fail_label));                    // let the interpreter convert dense arrays required as hashes
    }
    emitter.instruction("mov rax, rdi");                                        // move the unboxed heap pointer into the retained-result register
    abi::emit_incref_if_refcounted(emitter, &slot.ty.codegen_repr());
    emit_owned_slot_replacement(emitter, slot);
}

/// Validates and stores a boxed x86_64 eval object into an object property slot.
fn emit_x86_64_store_object_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                   // reload the boxed eval value for object type validation
        abi::emit_symbol_address(emitter, "rsi", &label);
        abi::emit_load_int_immediate(emitter, "rdx", len as i64);
        emitter.instruction("xor ecx, ecx");                                    // allow exact class matches for object property type hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emitter.instruction("test rax, rax");                                   // check whether the value satisfied the object property type hint
        emitter.instruction(&format!("je {}", fail_label));                     // reject values that fail the object property type hint
    }
    emit_x86_64_store_heap_property_slot(emitter, slot, 6, fail_label);
}

/// Stores a boxed eval value into an x86_64 nullable-int tagged-scalar property slot.
fn emit_x86_64_store_tagged_scalar_property(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    let null_label = format!(
        "{}_tagged_scalar_null_x",
        slot_body_label_raw(slot, "set")
    );
    let done_label = format!(
        "{}_tagged_scalar_done_x",
        slot_body_label_raw(slot, "set")
    );
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed eval value for nullable-int inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the assigned value tag and payload words
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("je {}", null_label));                         // materialize a tagged null for null property writes
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed eval value for integer coercion
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("jmp {}", done_label));                        // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer for the store
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); //store the nullable integer payload into the property slot
    emitter.instruction(
        &format!("mov QWORD PTR [r11 + {}], rdx", slot.offset + 8)
    );
}

/// Groups property slots by class id while preserving sorted class order.
fn grouped_slots(slots: &[EvalPropertySlot]) -> BTreeMap<u64, Vec<&EvalPropertySlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped
            .entry(slot.class_id)
            .or_insert_with(Vec::new)
            .push(slot);
    }
    grouped
}

/// Returns a platform-safe body label for a get/set property slot.
fn slot_body_label(module: &Module, slot: &EvalPropertySlot, mode: &str) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!("{}{}", slot_body_label_raw(slot, mode), suffix)
}

/// Returns a platform-safe label for continuing after a scoped property name miss.
fn slot_access_miss_label(module: &Module, slot: &EvalPropertySlot, mode: &str) -> String {
    format!("{}_access_miss", slot_body_label(module, slot, mode))
}

/// Returns a platform-safe label for a successful scoped property visibility check.
fn slot_scope_ok_label(module: &Module, slot: &EvalPropertySlot, mode: &str) -> String {
    format!("{}_scope_ok", slot_body_label(module, slot, mode))
}

/// Returns the architecture-independent body label stem for a property slot.
///
/// `mode` is a compiler-controlled literal and becomes part of the fixed prefix; the PHP names
/// go through `join_php_symbol()` so slots differing only in underscore placement stay distinct.
fn slot_body_label_raw(slot: &EvalPropertySlot, mode: &str) -> String {
    join_php_symbol(
        &format!("__elephc_eval_property_{}", mode),
        &[&slot.class_name, &slot.declaring_class, &slot.property],
    )
}

/// Returns class scopes that satisfy one member visibility for a declaring class.
fn visibility_scope_names(
    module: &Module,
    declaring_class: &str,
    visibility: &Visibility,
) -> Vec<String> {
    match visibility {
        Visibility::Public => Vec::new(),
        Visibility::Private => vec![declaring_class.to_string()],
        Visibility::Protected => related_class_scope_names(module, declaring_class),
    }
}

/// Returns AOT classes in the same inheritance line as `declaring_class`.
fn related_class_scope_names(module: &Module, declaring_class: &str) -> Vec<String> {
    let mut scopes = module
        .class_infos
        .keys()
        .filter(|class_name| {
            is_same_or_descendant(module, class_name, declaring_class)
                || is_same_or_descendant(module, declaring_class, class_name)
        })
        .cloned()
        .collect::<Vec<_>>();
    scopes.sort_by(|left, right| {
        class_id_for_scope(module, left)
            .cmp(&class_id_for_scope(module, right))
            .then_with(|| left.cmp(right))
    });
    scopes
}

/// Returns true when `class_name` is `ancestor` or descends from it.
fn is_same_or_descendant(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns the deterministic class id used to order generated scope checks.
fn class_id_for_scope(module: &Module, class_name: &str) -> u64 {
    module
        .class_infos
        .get(class_name)
        .map(|class_info| class_info.class_id)
        .unwrap_or(u64::MAX)
}


/// Emits a C-visible global label with target-specific symbol mangling.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    let symbol = module.target.extern_symbol(name);
    emitter.label_global(&symbol);
}
