//! Purpose:
//! Lowers enum case and enum method dispatch paths.
//! Shares receiver preparation and ABI call conventions with the object call dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Receiver ownership, late/static binding, and vtable slot layout must match class metadata emission.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::enum_case_symbol;
use crate::parser::ast::Expr;
use crate::types::{EnumCaseValue, EnumInfo, PhpType};

const ENUM_FROM_INVALID_BACKING_SUFFIX: &str = " is not a valid backing value for enum ";
const ENUM_FROM_INVALID_STRING_BACKING_SUFFIX: &str = "\" is not a valid backing value for enum ";

/// Lowers `EnumName::method(...)` by routing through builtin enum helpers.
pub(super) fn emit_enum_static_method_call(
    enum_name: &str,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("{}::{}()", enum_name, method));
    let Some(enum_info) = ctx.enums.get(enum_name).cloned() else {
        emitter.comment(&format!("WARNING: undefined enum {}", enum_name));
        return PhpType::Int;
    };

    match method {
        "cases" => emit_enum_cases(enum_name, &enum_info, emitter, ctx),
        "from" => emit_enum_from_like(enum_name, &enum_info, args, emitter, ctx, data, false),
        "tryfrom" => emit_enum_from_like(enum_name, &enum_info, args, emitter, ctx, data, true),
        _ => {
            emitter.comment(&format!("WARNING: undefined enum method {}::{}", enum_name, method));
            PhpType::Int
        }
    }
}

/// Emits the `EnumName::cases()` static method, which returns an array of all
/// enum case singleton objects in declaration order.
///
/// Each case singleton is loaded from the data section via `enum_case_symbol`
/// and stored into the payload of a newly allocated array. The array capacity
/// is set to the exact case count (or 4 for enums with no cases). The returned
/// type is `Array<Object(EnumName)>`.
fn emit_enum_cases(
    enum_name: &str,
    enum_info: &EnumInfo,
    emitter: &mut Emitter,
    _ctx: &mut Context,
) -> PhpType {
    let capacity = if enum_info.cases.is_empty() {
        4
    } else {
        enum_info.cases.len()
    };
    let result_reg = abi::int_result_reg(emitter);
    let array_ptr_reg = abi::symbol_scratch_reg(emitter);
    let len_reg = abi::temp_int_reg(emitter.target);
    let cap_reg = abi::int_arg_reg_name(emitter.target, 0);
    let elem_size_reg = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_load_int_immediate(emitter, cap_reg, capacity as i64);            // capacity = exact enum case count (or a small empty-array default)
    abi::emit_load_int_immediate(emitter, elem_size_reg, 8);                    // enum case arrays store one pointer per element
    abi::emit_call_label(emitter, "__rt_array_new");                            // allocate the enum cases array
    abi::emit_push_reg(emitter, result_reg);                                    // save the array pointer while filling elements

    for (i, case) in enum_info.cases.iter().enumerate() {
        let case_label = enum_case_symbol(enum_name, &case.name);
        abi::emit_load_symbol_to_reg(emitter, result_reg, &case_label, 0);      // load the enum singleton pointer from its slot through the target-aware symbol helper
        abi::emit_incref_if_refcounted(emitter, &PhpType::Object(enum_name.to_string())); // array storage becomes a new owner of the singleton reference
        abi::emit_load_temporary_stack_slot(emitter, array_ptr_reg, 0);         // peek the enum cases array pointer from the temporary stack slot
        if i == 0 {
            super::super::super::arrays::emit_array_value_type_stamp(
                emitter,
                array_ptr_reg,
                &PhpType::Object(enum_name.to_string()),
            );
        }
        abi::emit_store_to_address(emitter, result_reg, array_ptr_reg, 24 + i * 8); // store the enum singleton pointer in the array payload
        abi::emit_load_int_immediate(emitter, len_reg, (i + 1) as i64);        // materialize the updated array length after appending this enum case
        abi::emit_store_to_address(emitter, len_reg, array_ptr_reg, 0);         // persist the new enum cases array length
    }

    abi::emit_pop_reg(emitter, result_reg);                                     // pop the enum cases array pointer into the active integer result register
    PhpType::Array(Box::new(PhpType::Object(enum_name.to_string())))
}

/// Emits the `EnumName::from(value)` or `EnumName::tryFrom(value)` static method.
///
/// `from` throws a catchable `ValueError` if no case matches. `tryFrom` returns
/// `null` (boxed as `Void`) when no case matches and yields a `EnumName|Void`
/// union type. Backing type must be `Int` or `Str`. For `Str` backing, the input
/// string is preserved on the temporary stack across candidate comparisons and
/// cleaned up on both the success and "no match" paths.
fn emit_enum_from_like(
    enum_name: &str,
    enum_info: &EnumInfo,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    is_try: bool,
) -> PhpType {
    let Some(backing_ty) = enum_info.backing_type.as_ref() else {
        emitter.comment(&format!("WARNING: enum {} has no backing type", enum_name));
        return PhpType::Int;
    };
    let Some(arg) = args.first() else {
        emitter.comment(&format!(
            "WARNING: missing enum backing argument for {}::{}",
            enum_name,
            if is_try { "tryFrom" } else { "from" }
        ));
        return PhpType::Int;
    };

    let input_ty = emit_expr(arg, emitter, ctx, data);
    let success_label = ctx.next_label("enum_from_success");
    let done_label = ctx.next_label("enum_from_done");
    let result_reg = abi::int_result_reg(emitter);
    let string_ptr_reg = abi::string_result_regs(emitter).0;
    let string_len_reg = abi::string_result_regs(emitter).1;
    let string_cleanup_label = if matches!(backing_ty, PhpType::Str) {
        Some(ctx.next_label("enum_from_cleanup_input"))
    } else {
        None
    };

    match backing_ty {
        PhpType::Int => {
            let _ = input_ty;
            for case in &enum_info.cases {
                let Some(EnumCaseValue::Int(value)) = case.value.as_ref() else {
                    continue;
                };
                let next_label = ctx.next_label("enum_from_next");
                let case_value_reg = abi::temp_int_reg(emitter.target);
                load_immediate(emitter, case_value_reg, *value);                // materialize the current enum backing integer for comparison
                emitter.instruction(&format!("cmp {}, {}", result_reg, case_value_reg)); // compare the input integer with the current enum backing value
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction(&format!("b.ne {}", next_label));   // continue scanning when the current enum backing value does not match
                    }
                    Arch::X86_64 => {
                        emitter.instruction(&format!("jne {}", next_label));    // continue scanning when the current enum backing value does not match
                    }
                }
                let case_label = enum_case_symbol(enum_name, &case.name);
                abi::emit_load_symbol_to_reg(emitter, result_reg, &case_label, 0); // load the matching enum singleton pointer
                abi::emit_jump(emitter, &success_label);                        // return the matching enum singleton immediately
                emitter.label(&next_label);
            }
        }
        PhpType::Str => {
            abi::emit_push_reg_pair(emitter, string_ptr_reg, string_len_reg);   // preserve the input string payload across candidate comparisons
            for case in &enum_info.cases {
                let Some(EnumCaseValue::Str(value)) = case.value.as_ref() else {
                    continue;
                };
                let match_label = ctx.next_label("enum_from_case");
                let next_label = ctx.next_label("enum_from_next");
                let bytes = crate::string_bytes::literal_bytes(value);
                let (label, len) = data.add_string(&bytes);
                let (input_ptr_reg, input_len_reg, candidate_ptr_reg, candidate_len_reg) =
                    match emitter.target.arch {
                        Arch::AArch64 => ("x1", "x2", "x3", "x4"),
                        Arch::X86_64 => ("rdi", "rsi", "rdx", "rcx"),
                    };
                abi::emit_load_temporary_stack_slot(emitter, input_ptr_reg, 0); // reload the input string pointer into the first __rt_str_eq argument register for this candidate comparison
                abi::emit_load_temporary_stack_slot(emitter, input_len_reg, 8); // reload the input string length into the paired __rt_str_eq argument register for this candidate comparison
                abi::emit_symbol_address(emitter, candidate_ptr_reg, &label);   // materialize the candidate enum backing string address
                abi::emit_load_int_immediate(emitter, candidate_len_reg, len as i64); // materialize the candidate enum backing string length
                abi::emit_call_label(emitter, "__rt_str_eq");                    // compare the input string against the candidate backing string
                abi::emit_branch_if_int_result_nonzero(emitter, &match_label);   // branch when the current enum backing string matches
                abi::emit_jump(emitter, &next_label);                            // continue scanning when the current enum backing string does not match
                emitter.label(&match_label);
                let case_label = enum_case_symbol(enum_name, &case.name);
                abi::emit_load_symbol_to_reg(emitter, result_reg, &case_label, 0); // load the matching enum singleton pointer
                if let Some(cleanup_label) = &string_cleanup_label {
                    abi::emit_jump(emitter, cleanup_label);                     // drop the preserved input string before returning the match
                }
                emitter.label(&next_label);
            }
        }
        _ => {
            emitter.comment("WARNING: unsupported enum backing type in codegen");
            return PhpType::Int;
        }
    }

    if is_try {
        if matches!(backing_ty, PhpType::Str) {
            abi::emit_release_temporary_stack(emitter, 16);                     // drop the preserved input string payload before returning null
        }
        emit_null_into_x0(emitter);
        crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Void);
        abi::emit_jump(emitter, &done_label);                                   // return boxed null when tryFrom() does not match any case
    } else {
        emit_enum_from_value_error(enum_name, backing_ty, emitter, data);
    }

    if let Some(cleanup_label) = &string_cleanup_label {
        emitter.label(cleanup_label);
        abi::emit_release_temporary_stack(emitter, 16);                         // drop the preserved input string payload before returning the matching singleton
        abi::emit_jump(emitter, &success_label);                                // continue through the shared success path with a clean stack
    }

    emitter.label(&success_label);
    if is_try {
        crate::codegen::emit_box_current_value_as_mixed(
            emitter,
            &PhpType::Object(enum_name.to_string()),
        );
    }
    emitter.label(&done_label);
    if is_try {
        PhpType::Union(vec![PhpType::Object(enum_name.to_string()), PhpType::Void])
    } else {
        PhpType::Object(enum_name.to_string())
    }
}

/// Builds and throws the PHP-compatible `ValueError` for `Enum::from()` when
/// no declared case has the requested backing value. Leaves no enum-input
/// temporary stack storage behind before entering the exception unwinder.
fn emit_enum_from_value_error(
    enum_name: &str,
    backing_ty: &PhpType,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    match backing_ty {
        PhpType::Int => emit_enum_from_int_value_error_message(enum_name, emitter, data),
        PhpType::Str => emit_enum_from_string_value_error_message(enum_name, emitter, data),
        _ => return,
    }
    abi::emit_call_label(emitter, "__rt_str_persist");                          // copy the dynamically composed ValueError message into stable heap storage
    if matches!(backing_ty, PhpType::Str) {
        abi::emit_release_temporary_stack(emitter, 16);                         // drop the preserved unmatched string after the message has been copied
    }
    emit_throw_value_error_from_string_result(emitter);
}

/// Emits the dynamic error-message text for an unmatched integer-backed enum
/// value. The unmatched integer is still in the active integer result register
/// when this helper runs.
fn emit_enum_from_int_value_error_message(
    enum_name: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    abi::emit_call_label(emitter, "__rt_itoa");                                 // convert the unmatched backing integer to decimal text
    let suffix = format!("{}{}", ENUM_FROM_INVALID_BACKING_SUFFIX, enum_name);
    emit_concat_current_string_with_static_suffix(&suffix, emitter, data);
}

/// Emits the dynamic error-message text for an unmatched string-backed enum
/// value. The input string pointer and length are still preserved in the
/// temporary stack slot created before candidate scanning.
fn emit_enum_from_string_value_error_message(
    enum_name: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    emit_concat_static_prefix_with_preserved_string("\"", emitter, data);
    let suffix = format!(
        "{}{}",
        ENUM_FROM_INVALID_STRING_BACKING_SUFFIX,
        enum_name
    );
    emit_concat_current_string_with_static_suffix(&suffix, emitter, data);
}

/// Concatenates a static prefix with the preserved enum input string and leaves
/// the resulting string in the target's string-result registers.
fn emit_concat_static_prefix_with_preserved_string(
    prefix: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (prefix_label, prefix_len) = data.add_string(prefix.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(emitter, "x2", prefix_len as i64);
            abi::emit_load_temporary_stack_slot(emitter, "x3", 0);
            abi::emit_load_temporary_stack_slot(emitter, "x4", 8);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rax", &prefix_label);
            abi::emit_load_int_immediate(emitter, "rdx", prefix_len as i64);
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 8);
        }
    }
    abi::emit_call_label(emitter, "__rt_concat");                               // copy the static prefix and preserved input into the concat buffer
}

/// Concatenates the current string result with a static suffix and leaves the
/// resulting string in the target's string-result registers.
fn emit_concat_current_string_with_static_suffix(
    suffix: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (suffix_label, suffix_len) = data.add_string(suffix.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x3", &suffix_label);
            abi::emit_load_int_immediate(emitter, "x4", suffix_len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", &suffix_label);
            abi::emit_load_int_immediate(emitter, "rsi", suffix_len as i64);
        }
    }
    abi::emit_call_label(emitter, "__rt_concat");                               // append the static suffix to the current dynamic message prefix
}

/// Allocates a `ValueError` object using the current string result as its
/// message, publishes it in `_exc_value`, and enters the standard exception
/// unwinder. The current string must already be heap-persisted by the caller.
fn emit_throw_value_error_from_string_result(emitter: &mut Emitter) {
    let (message_ptr_reg, message_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, message_ptr_reg, message_len_reg);         // preserve the dynamic message while allocating the exception object
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x0", 32);                   // request Throwable payload storage
            abi::emit_call_label(emitter, "__rt_heap_alloc");                  // allocate the ValueError object payload
            emitter.instruction("mov x9, #6");                                  // heap kind 6 = throwable object instance
            emitter.instruction("str x9, [x0, #-8]");                           // stamp allocation as a runtime object
            abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_value_error_class_id", 0);
            emitter.instruction("str x9, [x0]");                                // store ValueError class id at object header
            abi::emit_load_temporary_stack_slot(emitter, "x9", 0);
            emitter.instruction("str x9, [x0, #8]");                            // store dynamic exception message pointer
            abi::emit_load_temporary_stack_slot(emitter, "x9", 8);
            emitter.instruction("str x9, [x0, #16]");                           // store dynamic exception message length
            emitter.instruction("str xzr, [x0, #24]");                          // exception code defaults to zero
            abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
            abi::emit_release_temporary_stack(emitter, 16);                     // release the preserved dynamic-message pair before unwinding
            abi::emit_jump(emitter, "__rt_throw_current");                     // enter the standard exception unwinder
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rax", 32);                  // request Throwable payload storage
            abi::emit_call_label(emitter, "__rt_heap_alloc");                  // allocate the ValueError object payload
            emitter.instruction("mov r10, 0x4548504c00000006");                 // x86_64 heap-kind word: HE LP magic + kind 6 object
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp allocation as a runtime object
            abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_value_error_class_id", 0);
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store ValueError class id at object header
            abi::emit_load_temporary_stack_slot(emitter, "r10", 0);
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store dynamic exception message pointer
            abi::emit_load_temporary_stack_slot(emitter, "r10", 8);
            emitter.instruction("mov QWORD PTR [rax + 16], r10");               // store dynamic exception message length
            emitter.instruction("mov QWORD PTR [rax + 24], 0");                 // exception code defaults to zero
            abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
            abi::emit_release_temporary_stack(emitter, 16);                     // release the preserved dynamic-message pair before unwinding
            abi::emit_jump(emitter, "__rt_throw_current");                     // enter the standard exception unwinder
        }
    }
}

/// Materialises the shared null sentinel value (a known non-null pointer bit
/// pattern) into the active integer result register. Used by `tryFrom` to
/// represent a null return before boxing it as `Void`.
fn emit_null_into_x0(emitter: &mut Emitter) {
    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        0x7fff_ffff_ffff_fffe_u64 as i64,
    ); // materialize the shared null sentinel in the active integer result register
}

/// Materialises a 64-bit signed integer immediate into the specified register
/// via `abi::emit_load_int_immediate`. Used only for enum backing value
/// comparisons in `emit_enum_from_like`.
fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    abi::emit_load_int_immediate(emitter, reg, value);                          // materialize the immediate through the shared target-aware helper
}
