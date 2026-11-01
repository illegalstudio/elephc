//! Purpose:
//! Shares target-aware value conversions used by instance and static property stores.
//! Keeps storage compatibility and refcount behavior identical across both property paths.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects` for declared instance-property writes.
//! - `crate::codegen::lower_inst::static_properties` for static-property writes.
//!
//! Key details:
//! - Mixed object and array payloads are retained before entering a concrete slot.
//! - Non-object Mixed payloads normalize to the null object sentinel.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::exceptions;

/// Returns true when a boxed Mixed value can be unboxed into an object property slot.
pub(super) fn can_unbox_mixed_to_object_property(
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && matches!(slot_ty.codegen_repr(), PhpType::Object(_))
}

/// Returns true when a boxed Mixed value can be unboxed into a callable property slot.
pub(super) fn can_unbox_mixed_to_callable_property(
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && matches!(slot_ty.codegen_repr(), PhpType::Callable)
}

/// Returns true when a boxed Mixed value can satisfy generic PHP array property storage.
pub(super) fn can_unbox_mixed_to_array_property(
    value_ty: &PhpType,
    slot_ty: &PhpType,
) -> bool {
    if !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return false;
    }
    match slot_ty.codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    }
}

/// Validates and materializes a boxed runtime array for an independently owned property slot.
///
/// Indexed and associative payloads are both legal PHP arrays. Their entries are widened to
/// boxed Mixed storage before the raw container pointer enters the declared property. An indexed
/// payload is promoted to a hash when the inferred slot representation specifically requires one.
pub(super) fn emit_mixed_array_for_property_store(
    ctx: &mut FunctionContext<'_>,
    slot_ty: &PhpType,
) {
    let indexed = ctx.next_label("prop_store_mixed_array_indexed");
    let associative = ctx.next_label("prop_store_mixed_array_associative");
    let done = ctx.next_label("prop_store_mixed_array_done");
    let target_is_assoc = matches!(slot_ty.codegen_repr(), PhpType::AssocArray { .. });

    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // runtime tag 4 identifies indexed PHP array storage
            ctx.emitter.instruction(&format!("b.eq {}", indexed));              // normalize indexed entries for the declared array slot
            ctx.emitter.instruction("cmp x0, #5");                              // runtime tag 5 identifies associative PHP array storage
            ctx.emitter.instruction(&format!("b.eq {}", associative));          // normalize associative entries for the declared array slot
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // runtime tag 4 identifies indexed PHP array storage
            ctx.emitter.instruction(&format!("je {}", indexed));                // normalize indexed entries for the declared array slot
            ctx.emitter.instruction("cmp rax, 5");                              // runtime tag 5 identifies associative PHP array storage
            ctx.emitter.instruction(&format!("je {}", associative));            // normalize associative entries for the declared array slot
        }
    }
    exceptions::emit_type_error(ctx, "Cannot assign non-array value to property of type array");

    ctx.emitter.label(&indexed);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // move the borrowed indexed payload into the container result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // move the borrowed indexed payload into the container result register
        }
    }
    if target_is_assoc {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {}
            Arch::X86_64 => ctx.emitter.instruction("mov rdi, rax"),            // pass the indexed payload to the SysV conversion helper
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {}
            Arch::X86_64 => ctx.emitter.instruction("mov rdi, rax"),            // pass the fresh owned hash to the Mixed-entry converter
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
    } else {
        abi::emit_incref_if_refcounted(
            ctx.emitter,
            &PhpType::Array(Box::new(PhpType::Mixed)),
        );
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("ldr x1, [x0, #-8]");                   // load the indexed payload's current element runtime tag
                ctx.emitter.instruction("lsr x1, x1, #8");                      // shift the element tag into the low bits
                ctx.emitter.instruction("and x1, x1, #0x7f");                   // discard heap-kind and copy-on-write metadata bits
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // pass the retained indexed payload to the SysV converter
                ctx.emitter.instruction("mov rsi, QWORD PTR [rax - 8]");        // load the indexed payload's current element runtime tag
                ctx.emitter.instruction("shr rsi, 8");                          // shift the element tag into the low bits
                ctx.emitter.instruction("and rsi, 0x7f");                       // discard heap-kind and copy-on-write metadata bits
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done)),       // skip the associative payload path
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done)),      // skip the associative payload path
    }

    ctx.emitter.label(&associative);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, x1"),                 // move the borrowed hash payload into the container result register
        Arch::X86_64 => ctx.emitter.instruction("mov rax, rdi"),                // move the borrowed hash payload into the container result register
    }
    abi::emit_incref_if_refcounted(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the retained hash payload to the SysV converter
    }
    abi::emit_call_label(ctx.emitter, "__rt_hash_to_mixed");
    ctx.emitter.label(&done);
}

/// Unboxes a Mixed property value into an independently retained object pointer.
///
/// Non-object payloads become the null object sentinel, matching the established
/// instance-property coercion. The caller remains responsible for releasing an
/// owning Mixed source after the retaining store has completed.
pub(super) fn emit_mixed_object_for_property_store(ctx: &mut FunctionContext<'_>) {
    let object_label = ctx.next_label("prop_store_mixed_value_object");
    let done = ctx.next_label("prop_store_mixed_value_done");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #6");                              // require the runtime object payload tag
            ctx.emitter.instruction(&format!("b.eq {}", object_label));         // store object payloads through the concrete slot path
            ctx.emitter.instruction("mov x0, #0");                              // normalize non-object payloads to the null sentinel
            ctx.emitter.instruction(&format!("b {}", done));                    // skip object-pointer promotion for non-object payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed object pointer into the result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 6");                              // require the runtime object payload tag
            ctx.emitter.instruction(&format!("je {}", object_label));           // store object payloads through the concrete slot path
            ctx.emitter.instruction("xor eax, eax");                            // normalize non-object payloads to the null sentinel
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip object-pointer promotion for non-object payloads
            ctx.emitter.label(&object_label);
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed object pointer into the result register
        }
    }
    ctx.emitter.label(&done);
    abi::emit_incref_if_refcounted(
        ctx.emitter,
        &PhpType::Object(String::new()), // retain the object independently for property storage
    );
}

/// Unboxes a Mixed property value into an independently retained callable descriptor.
///
/// A callable-typed property has to be reachable from a boxed value because PHP's own idiom puts
/// one there: `self::$make ??= self::make(...)` reads the property, boxes both the read and the
/// first-class callable so the two branches meet at one `Mixed`, and then assigns that back into
/// the `\Closure`-typed slot. Without this adapter the store had no representation to write and
/// refused, which is what `DependencyInjection\Container::get()` compiles to.
///
/// A descriptor slot holds one pointer, so the unboxed low word IS the value; it is retained the
/// same way a directly typed callable store retains it. Any other payload is a PHP `TypeError`,
/// not a silent null — assigning a non-callable to a `\Closure` property is an error in PHP too.
pub(super) fn emit_mixed_callable_for_property_store(ctx: &mut FunctionContext<'_>) {
    let callable_label = ctx.next_label("prop_store_mixed_value_callable");
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #10");                             // runtime tag 10 identifies a callable descriptor payload
            ctx.emitter.instruction(&format!("b.eq {}", callable_label));       // store descriptor payloads through the concrete slot path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 10");                             // runtime tag 10 identifies a callable descriptor payload
            ctx.emitter.instruction(&format!("je {}", callable_label));         // store descriptor payloads through the concrete slot path
        }
    }
    exceptions::emit_type_error(
        ctx,
        "Cannot assign non-callable value to property of type callable",
    );
    ctx.emitter.label(&callable_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // promote the unboxed descriptor pointer into the result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // promote the unboxed descriptor pointer into the result register
        }
    }
    crate::codegen_support::callable_descriptor::emit_retain_current_descriptor(ctx.emitter);
}
