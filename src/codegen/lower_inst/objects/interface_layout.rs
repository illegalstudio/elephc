//! Purpose:
//! Validates interface method symbols and computes object initialization offsets.
//!
//! Called from:
//! - The object lowering facade and sibling object support modules.
//!
//! Key details:
//! - Only layouts representable by emitted EIR method metadata are accepted.

use super::*;

/// Returns true when implemented interfaces require method-wrapper metadata not emitted here.
pub(super) fn class_interfaces_require_missing_method_symbols(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    class_info: &ClassInfo,
) -> bool {
    let emitted_methods = emitted_instance_method_keys(ctx);
    let mut seen = HashSet::default();
    let mut stack = class_info
        .interfaces
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    while let Some(interface_name) = stack.pop() {
        if !seen.insert(interface_name.to_string()) {
            continue;
        }
        let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
            return true;
        };
        if interface_requires_missing_method_symbol(
            ctx,
            interface_name,
            class_name,
            class_info,
            interface_info,
            emitted_methods,
        ) {
            return true;
        }
        stack.extend(interface_info.parents.iter().map(String::as_str));
    }
    false
}

/// Returns true when one interface table entry would point at an unavailable symbol.
pub(super) fn interface_requires_missing_method_symbol(
    ctx: &FunctionContext<'_>,
    interface_name: &str,
    fallback_class: &str,
    class_info: &ClassInfo,
    interface_info: &InterfaceInfo,
    emitted_methods: &crate::codegen::shared_state::EmittedMethods,
) -> bool {
    for method_name in &interface_info.method_order {
        if super::super::is_throwable_standard_method_key(method_name)
            && (super::super::is_throwable_like_class(ctx, fallback_class)
                || super::super::is_throwable_like_class(ctx, interface_name))
        {
            continue;
        }
        let impl_class = class_info
            .method_impl_classes
            .get(method_name)
            .map(String::as_str)
            .unwrap_or(fallback_class);
        if !emitted_methods.contains(impl_class, method_name)
            && IntrinsicCall::instance_method(impl_class, method_name)
                .and_then(|intrinsic| intrinsic.runtime_helper())
                .is_none()
        {
            return true;
        }
    }
    false
}

/// Borrows instance-method keys emitted by the EIR backend for contains-only validation.
pub(super) fn emitted_instance_method_keys<'ctx>(
    ctx: &'ctx FunctionContext<'_>,
) -> &'ctx crate::codegen::shared_state::EmittedMethods {
    ctx.shared.emitted_instance_method_keys()
}

/// Returns true when the current EIR module includes a class method body.
pub(super) fn class_method_already_emitted(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    ctx.shared
        .emitted_method_contains(class_name, method_key, is_static)
}

/// Collects property high-word offsets that should start with the typed-property sentinel.
pub(super) fn uninitialized_property_marker_offsets(class_info: &ClassInfo) -> Vec<usize> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, _))| {
            // Every physical slot without a schema default is PHP-uninitialized. Do not key
            // this on the name-level declared-property set: inherited private slots can be
            // intentionally absent from that visible map while still needing their own marker.
            //
            // KNOWN DIVERGENCE, deliberately left in place. Excluding owned REFERENCE slots is
            // wrong against PHP. Whether a slot is a reference is a WHOLE-PROGRAM property —
            // one `&$obj->prop` anywhere promotes that property for every instance of the class
            // — so a single reference somewhere unrelated makes the property born INITIALIZED:
            // `isset()` answers true, `get_object_vars()` lists it, and a read PHP refuses
            // returns the empty cell. Two classes identical apart from whether some later line
            // took a reference disagree about all three, where `php -n` 8.5.6 answers
            // `n | d | THROW` for both.
            //
            // Dropping the exclusion is NOT the fix, though it looks like a one-line one and
            // the slot has room (the cell pointer takes the low word, the marker loop writes
            // only the high word, and both reference write paths already zero it). Doing so
            // makes the guard fire for reference slots, and a caught uninitialized-property
            // fatal on a reference slot then corrupts an unrelated live heap string: a 9-byte
            // result came back with its first 8 bytes replaced by a pointer, deterministic in
            // shape and ASLR-varying in value, i.e. a freed block's free-list link. The same
            // program with an ordinary `throw new Error` is clean, and so is the same probe on
            // a NON-reference slot, so the fault is in what the reference-slot fatal frees on
            // the way out, not in the marker. Fix that release path first; the marker is one
            // line behind it.
            let is_owned_reference = class_info.property_slot_is_owned_reference(index);
            // An UNTYPED promoted property is born null in PHP, like any untyped property; its
            // schema default stays absent only so reflection keeps reporting no default. Marking
            // it made an object built without its constructor (`newInstanceWithoutConstructor()`,
            // which DeepClone uses) dump `uninitialized(mixed)` and count no properties.
            let untyped_promoted = class_info.promoted_properties.contains(property)
                && !class_info
                    .property_declared_slots
                    .get(index)
                    .copied()
                    .unwrap_or(false);
            let starts_uninitialized = class_info
                .defaults
                .get(index)
                .is_some_and(|default| default.is_none())
                && !is_owned_reference
                && !untyped_promoted;
            if starts_uninitialized {
                Some(8 + index * 16 + 8)
            } else {
                None
            }
        })
        .collect()
}

/// Collects the slot offsets of object-owned reference properties, whose ref-cells the
/// object allocates at construction and releases on destruction.
pub(super) fn owned_reference_property_offsets(
    class_info: &ClassInfo,
) -> Vec<(usize, PhpType, bool)> {
    class_info
        .properties
        .iter()
        .enumerate()
        .filter_map(|(index, (property, php_type))| {
            // Per SLOT, not per name: a child redeclaring an ancestor's private `$p` has two `p`
            // slots, and the name-keyed set describes only the child's. The ancestor's slot owns
            // its cell when the ancestor's `$r = &$this->p` promoted it, and a constructor that
            // skipped its allocation left a null cell for that method to write through.
            if class_info.property_slot_is_owned_reference(index) {
                let starts_uninitialized = class_info.property_slot_is_declared(index, property)
                    && class_info
                        .defaults
                        .get(index)
                        .is_some_and(|default| default.is_none());
                Some((8 + index * 16, php_type.clone(), starts_uninitialized))
            } else {
                None
            }
        })
        .collect()
}
