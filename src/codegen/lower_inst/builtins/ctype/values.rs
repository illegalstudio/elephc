//! Purpose:
//! Lowers the non-string arguments of PHP `ctype_*` builtins: ints, nullable ints, boxed
//! `mixed` values, and every statically known non-string type.
//!
//! Called from:
//! - `super::lower_ctype()` when the argument is not statically a string.
//!
//! Key details:
//! - php-src 8.1+ raises `Deprecated: ctype_x(): Argument of type T will be interpreted as
//!   string in the future` for every non-string argument, naming objects by their class.
//! - An int in 0..=255 is classified as that byte and an int in -128..=-1 as the byte
//!   `value + 256`. Any other int answers as its decimal string would: digits only for a
//!   non-negative value, a leading `-` for a negative one (`CtypeKind::accepts_*`).
//! - Every other non-string value answers `false`.
//! - A boxed value is peeled with `__rt_mixed_unbox`; a string payload jumps to the shared
//!   scan, so only string bytes ever reach the loop. The warning helper preserves the
//!   volatile registers, so the integer result register survives each deprecation.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::lower_inst::runtime_class_messages;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::ValueId;
use crate::types::PhpType;

use super::{emit_aarch64_ctype_predicate, emit_x86_64_ctype_predicate, CtypeKind, CtypeLabels};

/// Runtime value tag of an int payload.
const TAG_INT: u64 = 0;
/// Runtime value tag of a string payload.
const TAG_STRING: u64 = 1;
/// Runtime value tag of an object payload, named by its runtime class.
const TAG_OBJECT: u64 = 6;
/// Runtime value tag of the null payload a nullable-int tagged scalar can carry.
const TAG_NULL: u64 = 8;

/// Remaining runtime value tags, with the type name php-src's deprecation reports for them.
const STATIC_TAG_TYPE_NAMES: [(u64, &str); 7] = [
    (2, "float"),
    (3, "bool"),
    (4, "array"),
    (5, "array"),
    (8, "null"),
    (9, "resource"),
    (10, "Closure"),
];

/// Emits the deprecation and classification of a non-string argument.
///
/// Returns whether the caller must still emit the string scan, which a boxed value can reach
/// through `labels.string_entry`. Every path otherwise ends in a branch to the pass or fail
/// label of `labels`.
pub(super) fn emit_non_string_argument(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    value: ValueId,
    ty: &PhpType,
    labels: &CtypeLabels,
) -> Result<bool> {
    // A resource shares the int storage representation, so it is told apart before
    // `codegen_repr()` erases it.
    if matches!(ty, PhpType::Resource(_)) {
        return emit_static_type_failure(ctx, kind, "resource", labels);
    }
    match ty.codegen_repr() {
        PhpType::Int => {
            emit_deprecation(ctx, kind, "int");
            ctx.load_value_to_result(value)?;
            emit_int_classification(ctx, kind, labels);
            Ok(false)
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            emit_tagged_scalar_dispatch(ctx, kind, labels);
            Ok(false)
        }
        PhpType::Mixed => {
            ctx.load_value_to_result(value)?;
            emit_mixed_dispatch(ctx, kind, labels);
            Ok(true)
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            emit_object_deprecation(ctx, kind);
            abi::emit_jump(ctx.emitter, &labels.fail_label);
            Ok(false)
        }
        PhpType::Bool => emit_static_type_failure(ctx, kind, "bool", labels),
        PhpType::Float => emit_static_type_failure(ctx, kind, "float", labels),
        PhpType::Void => emit_static_type_failure(ctx, kind, "null", labels),
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emit_static_type_failure(ctx, kind, "array", labels)
        }
        PhpType::Callable => emit_static_type_failure(ctx, kind, "Closure", labels),
        other => Err(CodegenIrError::unsupported(format!(
            "{} for PHP type {:?}",
            kind.name(),
            other
        ))),
    }
}

/// Emits the deprecation for a statically known non-string type and answers `false`.
fn emit_static_type_failure(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    type_name: &str,
    labels: &CtypeLabels,
) -> Result<bool> {
    emit_deprecation(ctx, kind, type_name);
    abi::emit_jump(ctx.emitter, &labels.fail_label);
    Ok(false)
}

/// Emits php-src's complete "will be interpreted as string" deprecation for one type name.
fn emit_deprecation(ctx: &mut FunctionContext<'_>, kind: CtypeKind, type_name: &str) {
    runtime_class_messages::emit_static_diagnostic(
        ctx,
        &format!(
            "Deprecated: {}(): Argument of type {} will be interpreted as string in the future\n",
            kind.name(),
            type_name
        ),
        true,
    );
}

/// Emits the deprecation naming the runtime class of the object in the int result register.
fn emit_object_deprecation(ctx: &mut FunctionContext<'_>, kind: CtypeKind) {
    let result = abi::int_result_reg(ctx.emitter);
    runtime_class_messages::emit_static_diagnostic(
        ctx,
        &format!("Deprecated: {}(): Argument of type ", kind.name()),
        false,
    );
    runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, result, "object");
    let (name_ptr, name_len) = abi::string_result_regs(ctx.emitter);
    let (fragment_ptr, fragment_len) = runtime_class_messages::diagnostic_fragment_regs(ctx.emitter);
    if name_ptr != fragment_ptr {
        ctx.emitter.instruction(&format!("mov {fragment_ptr}, {name_ptr}"));    // hand the borrowed class-name pointer to the fragment writer
    }
    if name_len != fragment_len {
        ctx.emitter.instruction(&format!("mov {fragment_len}, {name_len}"));    // hand the class-name length to the fragment writer
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
    runtime_class_messages::emit_static_diagnostic(
        ctx,
        " will be interpreted as string in the future\n",
        true,
    );
}

/// Dispatches a nullable int held as `{payload, tag}` in the int result and tag registers.
fn emit_tagged_scalar_dispatch(ctx: &mut FunctionContext<'_>, kind: CtypeKind, labels: &CtypeLabels) {
    let null_label = ctx.next_label("ctype_tagged_null");
    let tag = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {tag}, #{TAG_NULL}"));        // does the nullable int carry the runtime null tag?
            ctx.emitter.instruction(&format!("b.eq {null_label}"));             // null takes php-src's non-int deprecation and fails
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {tag}, {TAG_NULL}"));         // does the nullable int carry the runtime null tag?
            ctx.emitter.instruction(&format!("je {null_label}"));               // null takes php-src's non-int deprecation and fails
        }
    }
    emit_deprecation(ctx, kind, "int");
    emit_int_classification(ctx, kind, labels);
    ctx.emitter.label(&null_label);
    emit_deprecation(ctx, kind, "null");
    abi::emit_jump(ctx.emitter, &labels.fail_label);
}

/// Unboxes the `mixed` cell in the int result register and dispatches on its runtime tag.
fn emit_mixed_dispatch(ctx: &mut FunctionContext<'_>, kind: CtypeKind, labels: &CtypeLabels) {
    let string_label = ctx.next_label("ctype_mixed_string");
    let int_label = ctx.next_label("ctype_mixed_int");
    let object_label = ctx.next_label("ctype_mixed_object");
    let unknown_label = ctx.next_label("ctype_mixed_unknown");
    let static_labels: Vec<(u64, &'static str, String)> = STATIC_TAG_TYPE_NAMES
        .iter()
        .map(|(tag, name)| (*tag, *name, ctx.next_label("ctype_mixed_type")))
        .collect();
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let arm = ctx.emitter.target.arch == Arch::AArch64;
    let (tag, payload, beq) = if arm { ("x0", "x1", "b.eq") } else { ("rax", "rdi", "je") };
    let imm = |value: u64| if arm { format!("#{value}") } else { value.to_string() };
    ctx.emitter.instruction(&format!("cmp {tag}, {}", imm(TAG_STRING)));        // runtime tag 1 = string payload
    ctx.emitter.instruction(&format!("{beq} {string_label}"));                  // strings are scanned byte by byte without a deprecation
    ctx.emitter.instruction(&format!("cmp {tag}, {}", imm(TAG_INT)));           // runtime tag 0 = int payload
    ctx.emitter.instruction(&format!("{beq} {int_label}"));                     // ints follow php-src's single-byte rules
    ctx.emitter.instruction(&format!("cmp {tag}, {}", imm(TAG_OBJECT)));        // runtime tag 6 = object payload
    ctx.emitter.instruction(&format!("{beq} {object_label}"));                  // objects are named by their runtime class
    for (value, _, label) in &static_labels {
        ctx.emitter.instruction(&format!("cmp {tag}, {}", imm(*value)));        // identify the payload kind for the deprecation wording
        ctx.emitter.instruction(&format!("{beq} {label}"));                     // report this payload kind and fail the predicate
    }
    abi::emit_jump(ctx.emitter, &unknown_label);

    ctx.emitter.label(&string_label);
    let (string_ptr, _) = abi::string_result_regs(ctx.emitter);
    if string_ptr != payload {
        ctx.emitter.instruction(&format!("mov {string_ptr}, {payload}"));       // move the unboxed string pointer into the scan register
    }
    abi::emit_jump(ctx.emitter, &labels.string_entry);

    let result = abi::int_result_reg(ctx.emitter);
    ctx.emitter.label(&int_label);
    ctx.emitter.instruction(&format!("mov {result}, {payload}"));               // keep the int payload in the register the warning preserves
    emit_deprecation(ctx, kind, "int");
    emit_int_classification(ctx, kind, labels);

    ctx.emitter.label(&object_label);
    ctx.emitter.instruction(&format!("mov {result}, {payload}"));               // the class lookup reads the bare object, not its box
    emit_object_deprecation(ctx, kind);
    abi::emit_jump(ctx.emitter, &labels.fail_label);

    for (_, type_name, label) in &static_labels {
        ctx.emitter.label(label);
        emit_deprecation(ctx, kind, type_name);
        abi::emit_jump(ctx.emitter, &labels.fail_label);
    }
    // `__rt_mixed_unbox` peels nested boxes and reports legacy shapes as null, so no other
    // tag is expected; a future tag still fails with php-src's generic wording.
    ctx.emitter.label(&unknown_label);
    emit_deprecation(ctx, kind, "mixed");
    abi::emit_jump(ctx.emitter, &labels.fail_label);
}

/// Classifies the int in the int result register and branches to the pass or fail label.
fn emit_int_classification(ctx: &mut FunctionContext<'_>, kind: CtypeKind, labels: &CtypeLabels) {
    let byte_label = ctx.next_label("ctype_int_byte");
    let hit_label = ctx.next_label("ctype_int_hit");
    let out_of_range_label = ctx.next_label("ctype_int_out_of_range");
    let digits = if kind.accepts_digit_string() { &labels.pass_label } else { &labels.fail_label };
    let negative = if kind.accepts_negative_digit_string() {
        &labels.pass_label
    } else {
        &labels.fail_label
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #255");                            // unsigned check for an int already in the byte range 0..=255
            ctx.emitter.instruction(&format!("b.ls {byte_label}"));             // classify 0..=255 as that byte
            ctx.emitter.instruction("add x5, x0, #128");                        // shift -128..=-1 onto 0..=127 for one unsigned range check
            ctx.emitter.instruction("cmp x5, #127");                            // check for a negative int php-src maps onto a byte
            ctx.emitter.instruction(&format!("b.hi {out_of_range_label}"));     // other ints answer as their decimal string would
            ctx.emitter.instruction("add x0, x0, #256");                        // map -128..=-1 onto the bytes 128..=255
            ctx.emitter.label(&byte_label);
            ctx.emitter.instruction("mov w4, w0");                              // place the byte where the shared predicate reads it
            emit_aarch64_ctype_predicate(ctx, kind, &hit_label, &labels.fail_label);
            ctx.emitter.label(&hit_label);
            ctx.emitter.instruction(&format!("b {}", labels.pass_label));       // the single byte satisfied the predicate
            ctx.emitter.label(&out_of_range_label);
            ctx.emitter.instruction("cmp x0, #0");                              // the decimal string starts with '-' only for a negative int
            ctx.emitter.instruction(&format!("b.lt {negative}"));               // php-src's answer for a '-'-prefixed digit string
            ctx.emitter.instruction(&format!("b {digits}"));                    // php-src's answer for an all-digit string
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 255");                            // unsigned check for an int already in the byte range 0..=255
            ctx.emitter.instruction(&format!("jbe {byte_label}"));              // classify 0..=255 as that byte
            ctx.emitter.instruction("lea r9, [rax + 128]");                     // shift -128..=-1 onto 0..=127 for one unsigned range check
            ctx.emitter.instruction("cmp r9, 127");                             // check for a negative int php-src maps onto a byte
            ctx.emitter.instruction(&format!("ja {out_of_range_label}"));       // other ints answer as their decimal string would
            ctx.emitter.instruction("add rax, 256");                            // map -128..=-1 onto the bytes 128..=255
            ctx.emitter.label(&byte_label);
            ctx.emitter.instruction("mov r8d, eax");                            // place the byte where the shared predicate reads it
            emit_x86_64_ctype_predicate(ctx, kind, &hit_label, &labels.fail_label);
            ctx.emitter.label(&hit_label);
            ctx.emitter.instruction(&format!("jmp {}", labels.pass_label));     // the single byte satisfied the predicate
            ctx.emitter.label(&out_of_range_label);
            ctx.emitter.instruction("test rax, rax");                           // the decimal string starts with '-' only for a negative int
            ctx.emitter.instruction(&format!("js {negative}"));                 // php-src's answer for a '-'-prefixed digit string
            ctx.emitter.instruction(&format!("jmp {digits}"));                  // php-src's answer for an all-digit string
        }
    }
}
