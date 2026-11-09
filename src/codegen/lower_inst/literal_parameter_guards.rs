//! Purpose:
//! Validates boxed false/null parameters before callable invokers erase their source type.
//! Builds catchable argument diagnostics from the selected descriptor and runtime value.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker::owned_value_args::coerce()`.
//!
//! Key details:
//! - The argument is borrowed; its invoker owns any temporary box and exception cleanup.
//! - A successful check returns the canonical zero word for false or null.
//! - Descriptor names remain dynamic because invoker bodies can share a signature.

use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;

use super::{abi, Arch};

/// Validates the boxed result and raises TypeError before the callable entry can run.
pub(in crate::codegen) fn emit_boxed_literal_parameter_guard(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    parameter: (usize, &str),
    null_only: bool,
    label_prefix: &str,
) {
    let accepted = format!("{label_prefix}_accepted");
    let boolean = format!("{label_prefix}_boolean");
    let tag_reg = abi::int_result_reg(emitter);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    let null_tag = crate::codegen_support::sentinels::TAGGED_SCALAR_TAG_NULL;
    let prefix = format!(
        "(): Argument #{} (${}) must be of type {}, ",
        parameter.0 + 1, parameter.1, if null_only { "null" } else { "false" },
    );

    // -- inspect the original boxed value before a literal ABI conversion --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    if null_only {
        branch_tag(emitter, tag_reg, null_tag, &accepted);
    }
    branch_tag(emitter, tag_reg, i64::from(super::objects::BOOL_TAG), &boolean);
    let refusals = super::objects::RUNTIME_TAGS.iter()
        .filter(|(tag, _)| *tag != super::objects::BOOL_TAG
            && (!null_only || i64::from(*tag) != null_tag))
        .map(|(tag, name)| (*tag, *name, format!("{label_prefix}_tag_{tag}")))
        .collect::<Vec<_>>();
    for (tag, _, label) in &refusals {
        branch_tag(emitter, tag_reg, i64::from(*tag), label);
    }
    refuse_static(emitter, data, descriptor_offset, ArgumentPrefix::Static(&prefix), "mixed");

    for (tag, name, label) in refusals {
        emitter.label(&label);
        if tag == super::objects::OBJECT_TAG {
            let fallback = data.add_string(b"object");
            super::runtime_class_messages::emit_class_name_lookup(
                emitter, payload, &fallback,
                &format!("{label_prefix}_object_fallback"),
                &format!("{label_prefix}_object_done"),
            );
            refuse_loaded_type(emitter, data, descriptor_offset, ArgumentPrefix::Static(&prefix));
        } else {
            refuse_static(emitter, data, descriptor_offset, ArgumentPrefix::Static(&prefix), name);
        }
    }

    // -- boolean payload zero is accepted only by the false declaration --
    emitter.label(&boolean);
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter), payload);
    if null_only {
        let false_value = format!("{label_prefix}_false");
        abi::emit_branch_if_int_result_zero(emitter, &false_value);
        refuse_static(emitter, data, descriptor_offset, ArgumentPrefix::Static(&prefix), "true");
        emitter.label(&false_value);
        refuse_static(emitter, data, descriptor_offset, ArgumentPrefix::Static(&prefix), "false");
    } else {
        abi::emit_branch_if_int_result_zero(emitter, &accepted);
        refuse_static(emitter, data, descriptor_offset, ArgumentPrefix::Static(&prefix), "true");
    }
    emitter.label(&accepted);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
}

/// Branches on a runtime Mixed tag without assuming the target's result register.
fn branch_tag(emitter: &mut Emitter, register: &str, tag: i64, label: &str) {
    // -- compare the unboxed tag with this literal guard arm --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {register}, #{tag}"));            // compare the source tag
            emitter.instruction(&format!("b.eq {label}"));                      // select the matching value category
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {register}, {tag}"));             // compare the source tag
            emitter.instruction(&format!("je {label}"));                        // select the matching value category
        }
    }
}

/// Where an argument error's `(): Argument #1 ($name) must be of type T, ` comes from.
#[derive(Clone, Copy)]
enum ArgumentPrefix<'a> {
    Static(&'a str),
    /// The invoker's name table at this frame offset, for the index the refusal pushed, and
    /// whether this module's frames can fill the Throwable's trace.
    Table(usize, bool),
    /// The prefix a variadic guard stored in [`VARIADIC_PREFIX`], and the trace flag.
    Global(bool),
}

/// Loads the pushed parameter's prefix from `[count, mask, (name)*, (prefix)*]`.
fn emit_table_prefix(emitter: &mut Emitter, table_offset: usize) {
    let (ptr, len) = abi::string_result_regs(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::load_at_offset(emitter, "x10", table_offset);
            emitter.instruction("ldr x9, [sp]");                                // load the pushed parameter index
            emitter.instruction("ldr x11, [x10]");                              // load the table's name count
            emitter.instruction("add x9, x9, x11");                             // skip the name rows
            emitter.instruction("add x10, x10, x9, lsl #4");                    // address this parameter's prefix row
            emitter.instruction(&format!("ldr {ptr}, [x10, #16]"));             // load the prefix pointer
            emitter.instruction(&format!("ldr {len}, [x10, #24]"));             // load the prefix length
        }
        Arch::X86_64 => {
            abi::load_at_offset(emitter, "r10", table_offset);
            emitter.instruction("mov r8, QWORD PTR [rsp]");                     // load the pushed parameter index
            emitter.instruction("add r8, QWORD PTR [r10]");                     // skip the name rows
            emitter.instruction("shl r8, 4");                                   // scale to a table row
            emitter.instruction("add r10, r8");                                 // address this parameter's prefix row
            emitter.instruction(&format!("mov {ptr}, QWORD PTR [r10 + 16]"));   // load the prefix pointer
            emitter.instruction(&format!("mov {len}, QWORD PTR [r10 + 24]"));   // load the prefix length
        }
    }
}

/// Starts an argument error with a statically known PHP type name.
fn refuse_static(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    prefix: ArgumentPrefix<'_>,
    type_name: &str,
) {
    let (label, len) = data.add_string(type_name.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);
    refuse_loaded_type(emitter, data, descriptor_offset, prefix);
}

/// Combines the runtime type, parameter identity, and descriptor name into an owned diagnostic.
fn refuse_loaded_type(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    prefix: ArgumentPrefix<'_>,
) {
    let (ptr, len) = abi::string_result_regs(emitter);
    let (right_ptr, right_len) = super::runtime_class_messages::concat_right_operand_regs(emitter);
    let (given, given_len) = data.add_string(b" given");
    abi::emit_symbol_address(emitter, right_ptr, &given);
    abi::emit_load_int_immediate(emitter, right_len, given_len as i64);
    abi::emit_call_label(emitter, "__rt_concat");

    abi::emit_reg_move(emitter, right_ptr, ptr);
    abi::emit_reg_move(emitter, right_len, len);
    match prefix {
        ArgumentPrefix::Static(prefix) => {
            let (prefix_label, prefix_len) = data.add_string(prefix.as_bytes());
            abi::emit_symbol_address(emitter, ptr, &prefix_label);
            abi::emit_load_int_immediate(emitter, len, prefix_len as i64);
        }
        ArgumentPrefix::Table(table_offset, _) => emit_table_prefix(emitter, table_offset),
        ArgumentPrefix::Global(_) => {
            abi::emit_load_symbol_to_reg(emitter, ptr, VARIADIC_PREFIX, 0);
            abi::emit_load_symbol_to_reg(emitter, len, VARIADIC_PREFIX, 8);
        }
    }
    abi::emit_call_label(emitter, "__rt_concat");

    abi::emit_reg_move(emitter, right_ptr, ptr);
    abi::emit_reg_move(emitter, right_len, len);
    let descriptor = abi::symbol_scratch_reg(emitter);
    abi::load_at_offset(emitter, descriptor, descriptor_offset);
    abi::emit_load_from_address(
        emitter, ptr, descriptor,
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_NAME_OFFSET,
    );
    abi::emit_load_from_address(
        emitter, len, descriptor,
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_NAME_LENGTH_OFFSET,
    );
    let (anonymous, anonymous_len) = data.add_string(b"{closure}");
    let fallback_ptr = abi::symbol_scratch_reg(emitter);
    let fallback_len = abi::secondary_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, fallback_ptr, &anonymous);
    abi::emit_load_int_immediate(emitter, fallback_len, anonymous_len as i64);
    // -- anonymous descriptors still identify their closure in argument errors --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {len}, #0"));                     // does the descriptor have a PHP name?
            emitter.instruction(&format!(
                "csel {ptr}, {fallback_ptr}, {ptr}, eq"
            ));                                                                 // select the closure name when absent
            emitter.instruction(&format!(
                "csel {len}, {fallback_len}, {len}, eq"
            ));                                                                 // select the matching byte length
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {len}, {len}"));                 // does the descriptor have a PHP name?
            emitter.instruction(&format!("cmovz {ptr}, {fallback_ptr}"));       // select the closure name when absent
            emitter.instruction(&format!("cmovz {len}, {fallback_len}"));       // select the matching byte length
        }
    }
    abi::emit_call_label(emitter, "__rt_concat");
    abi::emit_call_label(emitter, "__rt_str_persist");
    throw_type_error(
        emitter,
        matches!(prefix, ArgumentPrefix::Table(_, true) | ArgumentPrefix::Global(true)),
    );
}

/// Transfers the persisted diagnostic to a compact TypeError and the standard unwinder.
fn throw_type_error(emitter: &mut Emitter, capture_trace: bool) {
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr, len);
    // -- construct a managed Throwable while preserving the diagnostic --
    match emitter.target.arch {
        Arch::AArch64 => {
            crate::codegen_support::throwable_layout::emit_allocate(
                emitter, crate::codegen_support::throwable_layout::PAYLOAD_SIZE,
            );
            emitter.instruction("mov x9, #6");                                  // classify the allocation as a Throwable
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the object heap kind
            emitter.instruction("bl __rt_object_handle_acquire");               // acquire its PHP object handle
            abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_type_error_class_id", 0);
            emitter.instruction("str x9, [x0]");                                // identify the TypeError class
            abi::emit_pop_reg_pair(emitter, ptr, len);
            emitter.instruction("str x1, [x0, #8]");                            // transfer the persisted diagnostic pointer
            emitter.instruction("str x2, [x0, #16]");                           // store the diagnostic byte length
            emitter.instruction("str xzr, [x0, #24]");                          // initialize the exception code
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "x0");
            emitter.instruction(&format!(
                "str xzr, [x0, #{}]", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET
            ));
            if capture_trace {
                // php fills the trace at creation: Symfony's `PropertyAccessor` reads it.
                abi::emit_push_reg(emitter, "x0");
                abi::emit_call_label(emitter, "__rt_throwable_capture_trace");
                abi::emit_pop_reg(emitter, "x0");
            }
            abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
            emitter.instruction("b __rt_throw_current");                        // unwind argument owners before the PHP catch
        }
        Arch::X86_64 => {
            crate::codegen_support::throwable_layout::emit_allocate(
                emitter, crate::codegen_support::throwable_layout::PAYLOAD_SIZE,
            );
            abi::emit_load_int_immediate(
                emitter, "r10",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(6) as i64,
            );
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the canonical Throwable heap kind
            emitter.instruction("call __rt_object_handle_acquire");             // acquire its PHP object handle
            abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_type_error_class_id", 0);
            emitter.instruction("mov QWORD PTR [rax], r10");                    // identify the TypeError class
            let diagnostic_ptr = abi::symbol_scratch_reg(emitter);
            let diagnostic_len = abi::secondary_scratch_reg(emitter);
            abi::emit_pop_reg_pair(emitter, diagnostic_ptr, diagnostic_len);
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + 8], {diagnostic_ptr}"
            ));                                                                 // transfer the persisted diagnostic pointer
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + 16], {diagnostic_len}"
            ));                                                                 // store the diagnostic byte length
            emitter.instruction("mov QWORD PTR [rax + 24], 0");                 // initialize the exception code
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "rax");
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {}], 0", crate::codegen_support::throwable_layout::PREVIOUS_OFFSET
            ));
            if capture_trace {
                abi::emit_push_reg(emitter, "rax");
                abi::emit_call_label(emitter, "__rt_throwable_capture_trace");
                abi::emit_pop_reg(emitter, "rax");
            }
            abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
            emitter.instruction("jmp __rt_throw_current");                      // unwind argument owners before the PHP catch
        }
    }
}

/// The runtime tags a declared parameter takes from a boxed argument, and the name php gives it.
///
/// `None` leaves the declaration to the ordinary coercion: a nullable or union type accepts more
/// than one shape, and a non-numeric string is refused where the scalar is converted.
pub(in crate::codegen) fn declared_parameter_contract(
    target: &crate::types::PhpType,
) -> Option<(&'static [u8], String)> {
    use crate::types::PhpType;
    match target {
        PhpType::Object(class) => {
            let name = class.trim_start_matches('\\');
            Some((&[6, 10], if name.is_empty() { "object".to_string() } else { name.to_string() }))
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => Some((&[4, 5], "array".to_string())),
        PhpType::Int => Some((&[0, 1, 2, 3], "int".to_string())),
        PhpType::Float => Some((&[0, 1, 2, 3], "float".to_string())),
        PhpType::Bool => Some((&[0, 1, 2, 3], "bool".to_string())),
        PhpType::Str => Some((&[0, 1, 2, 3, 6], "string".to_string())),
        _ => None,
    }
}

/// Refuses a boxed argument its declared parameter cannot take, and leaves the box in place.
///
/// A direct call is checked where it is written. A call by name reaches the callee through its
/// invoker, which only converts, so `$o->$setter(null)` used to enter the setter: Symfony's
/// `PropertyAccessor` turns the `TypeError` php raises there into the error the form reports.
///
/// Bodies are shared between signatures that differ only in parameter names, so a body holds the
/// tag test alone: the message prefix sits in the invoker's name table and the refusal is one
/// routine for the whole program.
pub(in crate::codegen) fn emit_boxed_declared_parameter_guard(
    emitter: &mut Emitter,
    index: usize,
    accepted_tags: &[u8],
    numeric_strings: bool,
    table_reg: &str,
    label_prefix: &str,
) {
    let accepted = format!("{label_prefix}_accepted");
    let string = format!("{label_prefix}_string");
    let tag_reg = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for tag in accepted_tags {
        // An `int` or `float` parameter takes a string only when the WHOLE string is numeric
        // (php refuses `"x"`, `"7x"` and `"0x1A"` with a TypeError); the cast that follows would
        // have answered 0 for them.
        let target = if numeric_strings && *tag == STRING_TAG { &string } else { &accepted };
        branch_tag(emitter, tag_reg, i64::from(*tag), target);
    }
    abi::emit_load_int_immediate(emitter, table_reg, index as i64);
    abi::emit_call_label(emitter, DECLARED_PARAMETER_REFUSAL);
    if numeric_strings {
        emitter.label(&string);
        let (ptr, _) = abi::string_result_regs(emitter);
        let payload = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
        abi::emit_reg_move(emitter, ptr, payload);
        abi::emit_call_label(emitter, "__rt_str_looks_like_int_for_coercion");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cbnz {tag_reg}, {accepted}"));    // a numeric string converts
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("test {tag_reg}, {tag_reg}"));     // a numeric string converts
                emitter.instruction(&format!("jnz {accepted}"));                // keep it for the cast
            }
        }
        abi::emit_load_int_immediate(emitter, tag_reg, i64::from(STRING_TAG));
        abi::emit_load_int_immediate(emitter, table_reg, index as i64);
        abi::emit_call_label(emitter, DECLARED_PARAMETER_REFUSAL);
    }
    emitter.label(&accepted);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
}

/// The runtime tag of a string payload.
const STRING_TAG: u8 = 1;

/// Refuses a boxed element of a declared variadic collector (`int ...$xs`) that its element type
/// refuses, as the argument at its own position: `f(): Argument #2 must be of type int, string
/// given`. php names no parameter for a collected argument, and the position is only known when
/// the call runs, so the prefix is built then (on the refusal path only) and handed to the shared
/// [`VARIADIC_PARAMETER_REFUSAL`] through [`VARIADIC_PREFIX`]. The box stays in place.
#[allow(clippy::too_many_arguments)]
pub(in crate::codegen) fn emit_boxed_variadic_parameter_guard(
    emitter: &mut Emitter,
    data: &mut DataSection,
    accepted_tags: &[u8],
    numeric_strings: bool,
    type_name: &str,
    tail_index_reg: &str,
    position_bias: usize,
    label_prefix: &str,
) {
    let accepted = format!("{label_prefix}_accepted");
    let string = format!("{label_prefix}_string");
    let refuse = format!("{label_prefix}_refuse");
    let tag_reg = abi::int_result_reg(emitter);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for tag in accepted_tags {
        let target = if numeric_strings && *tag == STRING_TAG { &string } else { &accepted };
        branch_tag(emitter, tag_reg, i64::from(*tag), target);
    }
    abi::emit_jump(emitter, &refuse);
    if numeric_strings {
        emitter.label(&string);
        let (ptr, _) = abi::string_result_regs(emitter);
        abi::emit_push_reg(emitter, payload);
        abi::emit_reg_move(emitter, ptr, payload);
        abi::emit_call_label(emitter, "__rt_str_looks_like_int_for_coercion");
        abi::emit_pop_reg(emitter, payload);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cbnz {tag_reg}, {accepted}"));    // a numeric string converts
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("test {tag_reg}, {tag_reg}"));     // a numeric string converts
                emitter.instruction(&format!("jnz {accepted}"));                // keep it for the cast
            }
        }
        abi::emit_load_int_immediate(emitter, tag_reg, i64::from(STRING_TAG));
    }
    emitter.label(&refuse);
    // -- keep the refused tag and payload while the position's prefix is built --
    abi::emit_push_reg_pair(emitter, tag_reg, payload);
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter), tail_index_reg);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add x0, x0, #{position_bias}"));      // the argument's 1-based position
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add rax, {position_bias}"));          // the argument's 1-based position
        }
    }
    abi::emit_call_label(emitter, "__rt_itoa");
    let (ptr, len) = abi::string_result_regs(emitter);
    let (right_ptr, right_len) = super::runtime_class_messages::concat_right_operand_regs(emitter);
    abi::emit_reg_move(emitter, right_ptr, ptr);
    abi::emit_reg_move(emitter, right_len, len);
    let (head, head_len) = data.add_string(b"(): Argument #");
    abi::emit_symbol_address(emitter, ptr, &head);
    abi::emit_load_int_immediate(emitter, len, head_len as i64);
    abi::emit_call_label(emitter, "__rt_concat");
    let (tail, tail_len) = data.add_string(format!(" must be of type {type_name}, ").as_bytes());
    abi::emit_symbol_address(emitter, right_ptr, &tail);
    abi::emit_load_int_immediate(emitter, right_len, tail_len as i64);
    abi::emit_call_label(emitter, "__rt_concat");
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::emit_store_reg_to_symbol(emitter, ptr, VARIADIC_PREFIX, 0);
    abi::emit_store_reg_to_symbol(emitter, len, VARIADIC_PREFIX, 8);
    abi::emit_pop_reg_pair(emitter, tag_reg, payload);
    abi::emit_call_label(emitter, VARIADIC_PARAMETER_REFUSAL);
    emitter.label(&accepted);
    abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
}

/// Shared refusal for a declared variadic element: tag and payload in their registers, the
/// argument prefix in [`VARIADIC_PREFIX`], the invoker's frame still current.
pub(in crate::codegen) const VARIADIC_PARAMETER_REFUSAL: &str = "_eir_variadic_parameter_refusal";

/// The prefix the variadic guard built for [`VARIADIC_PARAMETER_REFUSAL`]: pointer, then length.
const VARIADIC_PREFIX: &str = "_eir_variadic_refusal_prefix";

/// Shared refusal: the unboxed tag and payload in their registers, the parameter index in
/// `table_reg`, the invoker's frame still current.
pub(in crate::codegen) const DECLARED_PARAMETER_REFUSAL: &str = "_eir_declared_parameter_refusal";

/// Emits [`DECLARED_PARAMETER_REFUSAL`], once per program.
pub(in crate::codegen) fn emit_declared_parameter_refusal(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    table_offset: usize,
    table_reg: &str,
    capture_trace: bool,
) {
    emit_parameter_refusal(
        emitter, data, descriptor_offset, table_reg, DECLARED_PARAMETER_REFUSAL,
        ArgumentPrefix::Table(table_offset, capture_trace),
    );
    data.add_comm(VARIADIC_PREFIX.to_string(), 16);
    emit_parameter_refusal(
        emitter, data, descriptor_offset, table_reg, VARIADIC_PARAMETER_REFUSAL,
        ArgumentPrefix::Global(capture_trace),
    );
}

/// One shared refusal routine: a type-name arm per runtime tag, then the message and the throw.
fn emit_parameter_refusal(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    table_reg: &str,
    routine: &str,
    prefix: ArgumentPrefix<'_>,
) {
    let tag_reg = abi::int_result_reg(emitter);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    emitter.blank();
    emitter.comment("shared parameter refusal");
    emitter.label_global(routine);
    abi::emit_push_reg(emitter, table_reg);
    let arms = super::objects::RUNTIME_TAGS.iter()
        .map(|(tag, name)| (*tag, *name, format!("{routine}_tag_{tag}")))
        .collect::<Vec<_>>();
    for (tag, _, label) in &arms {
        branch_tag(emitter, tag_reg, i64::from(*tag), label);
    }
    refuse_static(emitter, data, descriptor_offset, prefix, "mixed");
    for (tag, name, label) in arms {
        emitter.label(&label);
        if tag == super::objects::OBJECT_TAG {
            let fallback = data.add_string(b"object");
            super::runtime_class_messages::emit_class_name_lookup(
                emitter, payload, &fallback,
                &format!("{routine}_object_fallback"),
                &format!("{routine}_object_done"),
            );
            refuse_loaded_type(emitter, data, descriptor_offset, prefix);
        } else {
            refuse_static(emitter, data, descriptor_offset, prefix, name);
        }
    }
}
