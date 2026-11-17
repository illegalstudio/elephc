//! Purpose:
//! Lowers PHP `unpack()` calls for commonly used integer binary formats and
//! protocol, cache-metadata, and Unicode byte-processing code.
//!
//! Called from:
//! - `super::lower_builtin_call()` for the canonical `unpack` builtin name.
//!
//! Key details:
//! - Literal formats support `C`, `n`, `v`, `N`, `V`, and the 64-bit `J`, `P`, `Q`, including
//!   unnamed `*` repetition, named fields separated by `/`, and the optional byte offset.
//! - A lone unnamed field without a repeat count (`unpack('N', $bytes)[1]`, nette/utils
//!   `Strings::ord()`, brick/math `BigDecimal::fromFloatExact()`) is stored under the integer key
//!   `1`, exactly as php numbers an unnamed field.
//! - Repeated unnamed fields use PHP's one-based integer keys; named fixed
//!   fields use associative string keys.
//! - Both target paths return a boxed `array|false` representation and transfer
//!   the fresh hash owner into that box exactly once.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    CodegenIrError, Result,
};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// The `ValueError` php raises for a negative `$offset` or one past the end of `$data`.
const UNPACK_OFFSET_MESSAGE: &str =
    "unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)";

/// Byte order used by one supported integer field.
#[derive(Clone, Copy)]
enum ByteOrder {
    Native,
    Big,
    Little,
}

/// One fixed-width named field parsed from a literal format.
struct Field {
    width: usize,
    order: ByteOrder,
    key: FieldKey,
    byte_offset: usize,
    code: char,
}

/// Where one unpacked field is stored: under its format name, or under php's integer key `1`
/// for an unnamed field.
enum FieldKey {
    Name(String),
    Index(i64),
}

/// Supported lowering shape for one literal `unpack()` format.
enum FormatPlan {
    Sequence { width: i64, order: ByteOrder },
    Named(Vec<Field>),
}

/// Lowers one literal-format `unpack()` call into target-aware hash construction.
pub(crate) fn lower_unpack(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    super::ensure_arg_count_between(inst, "unpack", 2, 3)?;
    let format = expect_operand(inst, 0)?;
    let input = expect_operand(inst, 1)?;
    let offset = inst.operands.get(2).copied();
    let literal = const_string_operand(ctx, format)?.ok_or_else(|| {
        CodegenIrError::unsupported("unpack() currently requires a literal integer format")
    })?;
    match parse_format(&literal)? {
        FormatPlan::Sequence { width, order } => {
            lower_sequence(ctx, input, offset, width, order)?;
        }
        FormatPlan::Named(fields) => lower_named(ctx, input, offset, &fields)?,
    }
    store_if_result(ctx, inst)
}

/// Parses the supported integer subset of PHP's `unpack()` format language.
fn parse_format(format: &str) -> Result<FormatPlan> {
    if let Some(code) = format.strip_suffix('*') {
        if code.len() == 1 {
            let (width, order) = integer_directive(code.as_bytes()[0])?;
            if width > 4 {
                return Err(CodegenIrError::unsupported(
                    "unpack() 64-bit directives do not support unnamed `*` repetition",
                ));
            }
            return Ok(FormatPlan::Sequence {
                width: width as i64,
                order,
            });
        }
    }

    let mut fields = Vec::new();
    let mut byte_offset = 0usize;
    for component in format.split('/') {
        let bytes = component.as_bytes();
        let Some((&directive, name)) = bytes.split_first() else {
            return Err(CodegenIrError::unsupported("unpack() empty format component"));
        };
        let (width, order) = integer_directive(directive)?;
        // A repeater (`V2a`, `V1`): `count` consecutive fields. php names them `a1`, `a2`, ...
        // -- or, for a single repetition, just `a` -- and numbers unnamed ones from `1`. A `*`
        // repeater depends on the input length, which a literal-format lowering cannot size.
        let digits = name.iter().take_while(|byte| byte.is_ascii_digit()).count();
        if name.first() == Some(&b'*') {
            return Err(CodegenIrError::unsupported(format!(
                "unpack() unsupported repeated format component {component:?}"
            )));
        }
        let repetitions: usize = if digits == 0 {
            1
        } else {
            std::str::from_utf8(&name[..digits])
                .ok()
                .and_then(|text| text.parse().ok())
                .filter(|count| *count >= 1)
                .ok_or_else(|| {
                    CodegenIrError::unsupported(format!(
                        "unpack() unsupported repeated format component {component:?}"
                    ))
                })?
        };
        let name = &name[digits..];
        let base = if name.is_empty() {
            None
        } else {
            Some(
                std::str::from_utf8(name)
                    .map_err(|_| CodegenIrError::unsupported("unpack() format names must be UTF-8"))?
                    .to_string(),
            )
        };
        for repetition in 0..repetitions {
            let key = match &base {
                None => FieldKey::Index(repetition as i64 + 1),
                Some(base) if repetitions == 1 => FieldKey::Name(base.clone()),
                Some(base) => FieldKey::Name(format!("{}{}", base, repetition + 1)),
            };
            fields.push(Field {
                width,
                order,
                key,
                byte_offset,
                code: char::from(directive),
            });
            byte_offset += width;
        }
    }
    if fields.is_empty() {
        return Err(CodegenIrError::unsupported("unpack() empty format"));
    }
    Ok(FormatPlan::Named(fields))
}

/// Maps one PHP integer directive to its byte width and ordering.
fn integer_directive(directive: u8) -> Result<(usize, ByteOrder)> {
    match directive {
        b'C' => Ok((1, ByteOrder::Native)),
        b'n' => Ok((2, ByteOrder::Big)),
        b'N' => Ok((4, ByteOrder::Big)),
        b'V' => Ok((4, ByteOrder::Little)),
        b'v' => Ok((2, ByteOrder::Little)),
        b'J' => Ok((8, ByteOrder::Big)),
        b'P' | b'Q' => Ok((8, ByteOrder::Little)),
        other => Err(CodegenIrError::unsupported(format!(
            "unpack() integer format directive {:?}",
            char::from(other)
        ))),
    }
}

/// Calls the shared runtime loop for an unnamed `*` integer sequence.
fn lower_sequence(
    ctx: &mut FunctionContext<'_>,
    input: ValueId,
    offset: Option<ValueId>,
    width: i64,
    order: ByteOrder,
) -> Result<()> {
    let order = match order {
        ByteOrder::Big => 1,
        ByteOrder::Little | ByteOrder::Native => 0,
    };
    let spilled = spill_mixed_offset(ctx, offset)?;
    let (error, in_range) = (ctx.next_label("unpack_offset_error"), ctx.next_label("unpack_offset_ok"));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            super::strings::load_value_as_string_to_regs(ctx, input, "unpack", "x0", "x1")?;
            if !spilled { abi::emit_push_reg_pair(ctx.emitter, "x0", "x1"); }
            load_offset(ctx, offset, "x2", spilled)?;
            if !spilled { abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1"); }
            ctx.emitter.instruction("cmp x2, #0");                             // reject negative input offsets
            ctx.emitter.instruction(&format!("b.lt {error}"));                 // a negative offset raises php's ValueError
            ctx.emitter.instruction("cmp x2, x1");                             // compare offset with input length
            ctx.emitter.instruction(&format!("b.ls {in_range}"));              // an offset inside the string is usable
            ctx.emitter.label(&error);
            super::super::exceptions::emit_value_error(ctx, UNPACK_OFFSET_MESSAGE);
            ctx.emitter.label(&in_range);
            abi::emit_load_int_immediate(ctx.emitter, "x3", width);
            abi::emit_load_int_immediate(ctx.emitter, "x4", order);
        }
        Arch::X86_64 => {
            super::strings::load_value_as_string_to_regs(ctx, input, "unpack", "rdi", "rsi")?;
            if !spilled { abi::emit_push_reg_pair(ctx.emitter, "rdi", "rsi"); }
            load_offset(ctx, offset, "rdx", spilled)?;
            if !spilled { abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi"); }
            ctx.emitter.instruction("cmp rdx, 0");                             // reject negative input offsets
            ctx.emitter.instruction(&format!("jl {error}"));                   // a negative offset raises php's ValueError
            ctx.emitter.instruction("cmp rdx, rsi");                           // compare offset with input length
            ctx.emitter.instruction(&format!("jbe {in_range}"));               // an offset inside the string is usable
            ctx.emitter.label(&error);
            super::super::exceptions::emit_value_error(ctx, UNPACK_OFFSET_MESSAGE);
            ctx.emitter.label(&in_range);
            abi::emit_load_int_immediate(ctx.emitter, "rcx", width);
            abi::emit_load_int_immediate(ctx.emitter, "r8", order);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_unpack_integer_sequence");
    box_hash_or_false(ctx);
    Ok(())
}

/// Emits one statically-described named integer hash.
fn lower_named(
    ctx: &mut FunctionContext<'_>,
    input: ValueId,
    offset: Option<ValueId>,
    fields: &[Field],
) -> Result<()> {
    let required = fields
        .last()
        .map(|field| field.byte_offset + field.width)
        .unwrap_or(0);
    let fail = ctx.next_label("unpack_named_false");
    let done = ctx.next_label("unpack_named_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_named_aarch64(ctx, input, offset, fields, required, &fail)?,
        Arch::X86_64 => lower_named_x86_64(ctx, input, offset, fields, required, &fail)?,
    }
    box_owned_hash(ctx);
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&fail);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits bounds checking and named integer inserts for AArch64.
fn lower_named_aarch64(
    ctx: &mut FunctionContext<'_>,
    input: ValueId,
    offset: Option<ValueId>,
    fields: &[Field],
    required: usize,
    fail: &str,
) -> Result<()> {
    let spilled = spill_mixed_offset(ctx, offset)?;
    super::strings::load_value_as_string_to_regs(ctx, input, "unpack", "x9", "x10")?;
    load_offset(ctx, offset, "x11", spilled)?;
    let (error, in_range) = (ctx.next_label("unpack_offset_error"), ctx.next_label("unpack_offset_ok"));
    ctx.emitter.instruction("cmp x11, #0");                                    // reject negative input offsets
    ctx.emitter.instruction(&format!("b.lt {error}"));                         // a negative offset raises php's ValueError
    ctx.emitter.instruction("cmp x11, x10");                                  // compare offset with input length
    ctx.emitter.instruction(&format!("b.ls {in_range}"));                      // an offset inside the string is usable
    ctx.emitter.label(&error);
    super::super::exceptions::emit_value_error(ctx, UNPACK_OFFSET_MESSAGE);
    ctx.emitter.label(&in_range);
    ctx.emitter.instruction("sub x12, x10, x11");                             // remaining input bytes after the offset
    abi::emit_load_int_immediate(ctx.emitter, "x13", required as i64);
    ctx.emitter.instruction("cmp x12, x13");                                  // ensure all fixed fields fit
    let fits = ctx.next_label("unpack_fields_fit");
    ctx.emitter.instruction(&format!("b.hs {fits}"));                          // every fixed field fits the remaining input
    emit_truncation_warning(ctx, fields, fail);
    ctx.emitter.label(&fits);
    ctx.emitter.instruction("add x9, x9, x11");                              // cursor = input + offset
    ctx.emitter.instruction("sub sp, sp, #32");                               // reserve aligned cursor/hash spill slots
    ctx.emitter.instruction("str x9, [sp]");                                  // preserve the input cursor across runtime calls
    abi::emit_load_int_immediate(ctx.emitter, "x0", (fields.len() * 2).max(16) as i64);
    abi::emit_load_int_immediate(ctx.emitter, "x1", 0);
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    ctx.emitter.instruction("str x0, [sp, #8]");                              // retain the possibly-grown result hash
    for field in fields {
        ctx.emitter.instruction("ldr x9, [sp]");                              // reload the stable input cursor
        emit_integer_load_aarch64(ctx, field);
        emit_field_key_aarch64(ctx, field);
        ctx.emitter.instruction("mov x4, #0");                                // integer values have no high payload word
        ctx.emitter.instruction("mov x5, #0");                                // hash value tag 0 = integer
        ctx.emitter.instruction("ldr x0, [sp, #8]");                          // reload the current hash pointer
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("str x0, [sp, #8]");                          // preserve growth across the next insert
    }
    ctx.emitter.instruction("ldr x0, [sp, #8]");                              // publish the completed hash
    ctx.emitter.instruction("add sp, sp, #32");                               // release the aligned spill area
    Ok(())
}

/// Loads one field's hash key into `x1`/`x2`: a string pointer and length, or an integer key
/// (`x2 == -1`) for an unnamed field.
fn emit_field_key_aarch64(ctx: &mut FunctionContext<'_>, field: &Field) {
    match &field.key {
        FieldKey::Name(name) => {
            let (label, len) = ctx.data.add_string(name.as_bytes());
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        FieldKey::Index(index) => {
            abi::emit_load_int_immediate(ctx.emitter, "x1", *index);
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
        }
    }
}

/// Loads one field's hash key into `rsi`/`rdx`: a string pointer and length, or an integer key
/// (`rdx == -1`) for an unnamed field.
fn emit_field_key_x86_64(ctx: &mut FunctionContext<'_>, field: &Field) {
    match &field.key {
        FieldKey::Name(name) => {
            let (label, len) = ctx.data.add_string(name.as_bytes());
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
        FieldKey::Index(index) => {
            abi::emit_load_int_immediate(ctx.emitter, "rsi", *index);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
        }
    }
}

/// Loads one unsigned integer field into AArch64 hash value register `x3`.
fn emit_integer_load_aarch64(ctx: &mut FunctionContext<'_>, field: &Field) {
    let address = format!("[x9, #{}]", field.byte_offset);
    match field.width {
        1 => ctx.emitter.instruction(&format!("ldrb w3, {address}")),
        2 => {
            ctx.emitter.instruction(&format!("ldrh w3, {address}"));
            if matches!(field.order, ByteOrder::Big) {
                ctx.emitter.instruction("rev16 w3, w3");                       // convert network-order 16-bit input to host order
            }
        }
        4 => {
            ctx.emitter.instruction(&format!("ldr w3, {address}"));
            if matches!(field.order, ByteOrder::Big) {
                ctx.emitter.instruction("rev w3, w3");                         // convert network-order 32-bit input to host order
            }
        }
        8 => {
            ctx.emitter.instruction(&format!("ldr x3, {address}"));
            if matches!(field.order, ByteOrder::Big) {
                ctx.emitter.instruction("rev x3, x3");                         // convert network-order 64-bit input to host order
            }
        }
        _ => unreachable!("validated unpack integer width"),
    }
}

/// Emits bounds checking and named integer inserts for x86_64.
fn lower_named_x86_64(
    ctx: &mut FunctionContext<'_>,
    input: ValueId,
    offset: Option<ValueId>,
    fields: &[Field],
    required: usize,
    fail: &str,
) -> Result<()> {
    let spilled = spill_mixed_offset(ctx, offset)?;
    super::strings::load_value_as_string_to_regs(ctx, input, "unpack", "r10", "r11")?;
    load_offset(ctx, offset, "rax", spilled)?;
    let (error, in_range) = (ctx.next_label("unpack_offset_error"), ctx.next_label("unpack_offset_ok"));
    ctx.emitter.instruction("cmp rax, 0");                                     // reject negative input offsets
    ctx.emitter.instruction(&format!("jl {error}"));                           // a negative offset raises php's ValueError
    ctx.emitter.instruction("cmp rax, r11");                                  // compare offset with input length
    ctx.emitter.instruction(&format!("jbe {in_range}"));                       // an offset inside the string is usable
    ctx.emitter.label(&error);
    super::super::exceptions::emit_value_error(ctx, UNPACK_OFFSET_MESSAGE);
    ctx.emitter.label(&in_range);
    ctx.emitter.instruction("mov rdx, r11");                                  // compute remaining input byte count
    ctx.emitter.instruction("sub rdx, rax");                                  // remaining = input_len - offset
    ctx.emitter.instruction(&format!("cmp rdx, {required}"));                  // ensure every fixed field fits
    let fits = ctx.next_label("unpack_fields_fit");
    ctx.emitter.instruction(&format!("jae {fits}"));                           // every fixed field fits the remaining input
    emit_truncation_warning(ctx, fields, fail);
    ctx.emitter.label(&fits);
    ctx.emitter.instruction("add r10, rax");                                  // cursor = input + offset
    ctx.emitter.instruction("sub rsp, 32");                                   // reserve aligned cursor/hash spill slots
    ctx.emitter.instruction("mov QWORD PTR [rsp], r10");                      // preserve the input cursor across runtime calls
    abi::emit_load_int_immediate(ctx.emitter, "rdi", (fields.len() * 2).max(16) as i64);
    abi::emit_load_int_immediate(ctx.emitter, "rsi", 0);
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");                  // retain the possibly-grown result hash
    for field in fields {
        ctx.emitter.instruction("mov r10, QWORD PTR [rsp]");                  // reload the stable input cursor
        emit_integer_load_x86_64(ctx, field);
        emit_field_key_x86_64(ctx, field);
        ctx.emitter.instruction("mov r8, 0");                                 // integer values have no high payload word
        ctx.emitter.instruction("mov r9, 0");                                 // hash value tag 0 = integer
        ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");              // reload the current hash pointer
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
        ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rax");              // preserve growth across the next insert
    }
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 8]");                  // publish the completed hash
    ctx.emitter.instruction("add rsp, 32");                                   // release the aligned spill area
    Ok(())
}

/// Loads one unsigned integer field into x86_64 hash value register `rcx`.
fn emit_integer_load_x86_64(ctx: &mut FunctionContext<'_>, field: &Field) {
    let address = format!("[r10 + {}]", field.byte_offset);
    match field.width {
        1 => ctx.emitter.instruction(&format!("movzx ecx, BYTE PTR {address}")),
        2 => {
            ctx.emitter.instruction(&format!("movzx ecx, WORD PTR {address}"));
            if matches!(field.order, ByteOrder::Big) {
                ctx.emitter.instruction("rol cx, 8");                          // convert network-order 16-bit input to host order
            }
        }
        4 => {
            ctx.emitter.instruction(&format!("mov ecx, DWORD PTR {address}"));
            if matches!(field.order, ByteOrder::Big) {
                ctx.emitter.instruction("bswap ecx");                          // convert network-order 32-bit input to host order
            }
        }
        8 => {
            ctx.emitter.instruction(&format!("mov rcx, QWORD PTR {address}"));
            if matches!(field.order, ByteOrder::Big) {
                ctx.emitter.instruction("bswap rcx");                          // convert network-order 64-bit input to host order
            }
        }
        _ => unreachable!("validated unpack integer width"),
    }
}

/// Boxes a fresh associative integer hash, consuming its initial owner.
fn box_owned_hash(ctx: &mut FunctionContext<'_>) {
    emit_box_current_owned_value_as_mixed(ctx.emitter, &unpack_hash_type());
}

/// Boxes a runtime sequence hash or the boolean false fallback.
fn box_hash_or_false(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("unpack_sequence_false");
    let done_label = ctx.next_label("unpack_sequence_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("cbz x0, {false_label}")),
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {false_label}"));
        }
    }
    box_owned_hash(ctx);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&done_label);
}

/// Returns the associative runtime type shared by named and numeric unpack results.
fn unpack_hash_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Int),
    }
}

/// Converts a boxed (`mixed`) `$offset` to an int and parks it on the temporary stack.
///
/// The raw operand of a boxed offset is the box pointer, which the bounds checks would read as
/// a huge integer. The conversion calls the runtime, so it runs before the input string is
/// loaded into registers; `load_offset` pops the parked value afterwards. Returns whether a
/// value was parked.
fn spill_mixed_offset(ctx: &mut FunctionContext<'_>, offset: Option<ValueId>) -> Result<bool> {
    let Some(offset) = offset else {
        return Ok(false);
    };
    if !matches!(
        ctx.value_php_type(offset)?.codegen_repr(),
        PhpType::Mixed | PhpType::Union(_) | PhpType::Str | PhpType::Float
    ) {
        return Ok(false);
    }
    // Weak mode: a numeric string or a float reaches the integer parameter converted, as php does.
    super::super::resolve_int_operand_to_result(ctx, offset, "unpack offset")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(true)
}

/// Emits php's "not enough input values" warning for the first field the input cannot hold.
///
/// The remaining byte count is in `x12` (AArch64) or `rdx` (x86_64). Fields are grouped by
/// directive; the failing group is the first whose end passes the remaining bytes, and the
/// bytes php reports as provided are the remainder left after the whole fields before the
/// failure. The warning is followed by a jump to `fail`, which answers `false`.
fn emit_truncation_warning(ctx: &mut FunctionContext<'_>, fields: &[Field], fail: &str) {
    let mut groups: Vec<(char, usize, usize, usize)> = Vec::new(); // (code, start, width, count)
    for field in fields {
        match groups.last_mut() {
            Some((code, _, _, count)) if *code == field.code => *count += 1,
            _ => groups.push((field.code, field.byte_offset, field.width, 1)),
        }
    }
    let labels: Vec<String> = groups.iter().map(|_| ctx.next_label("unpack_truncated_group")).collect();
    for ((_, start, width, count), label) in groups.iter().zip(&labels) {
        let end = (start + width * count) as i64;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(ctx.emitter, "x13", end);
                ctx.emitter.instruction("cmp x12, x13");                       // does this directive group run past the remaining input?
                ctx.emitter.instruction(&format!("b.lo {label}"));             // the first such group holds the truncated field
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp rdx, {end}"));           // does this directive group run past the remaining input?
                ctx.emitter.instruction(&format!("jb {label}"));               // the first such group holds the truncated field
            }
        }
    }
    for ((code, start, width, _), label) in groups.iter().zip(&labels) {
        ctx.emitter.label(label);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_load_int_immediate(ctx.emitter, "x14", *start as i64);
                ctx.emitter.instruction("sub x14, x12, x14");                  // bytes left at the start of this group
                abi::emit_load_int_immediate(ctx.emitter, "x15", *width as i64);
                ctx.emitter.instruction("udiv x16, x14, x15");                 // whole fields that still fit
                ctx.emitter.instruction("msub x14, x16, x15, x14");            // bytes left at the truncated field
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rax, rdx");                       // copy the remaining byte count
                ctx.emitter.instruction(&format!("sub rax, {start}"));         // bytes left at the start of this group
                ctx.emitter.instruction("xor edx, edx");                       // clear the dividend high half
                ctx.emitter.instruction(&format!("mov rcx, {width}"));         // field width as the divisor
                ctx.emitter.instruction("div rcx");                            // rdx = bytes left at the truncated field
                ctx.emitter.instruction("mov rax, rdx");                       // stage the provided byte count
            }
        }
        let provided = if ctx.emitter.target.arch == Arch::AArch64 { "x14" } else { "rax" };
        abi::emit_push_reg(ctx.emitter, provided);
        emit_unpack_fragment(
            ctx,
            &format!("Warning: unpack(): Type {code}: not enough input values, need {width} values but only "),
            false,
        );
        abi::emit_load_temporary_stack_slot(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_call_label(ctx.emitter, "__rt_itoa");
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction("mov rdi, rax");                           // pass the digits pointer as the fragment pointer
            ctx.emitter.instruction("mov rsi, rdx");                           // pass the digits length as the fragment length
        }
        abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
        emit_unpack_fragment(ctx, " were provided\n", true);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_jump(ctx.emitter, fail);
    }
}

/// Emits one static diagnostic fragment; `complete` ends the warning line.
fn emit_unpack_fragment(ctx: &mut FunctionContext<'_>, text: &str, complete: bool) {
    let (label, len) = ctx.data.add_string(text.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(
        ctx.emitter,
        if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" },
    );
}

/// Loads the optional third argument, whose PHP default is byte offset zero.
///
/// `spilled` means `spill_mixed_offset` already parked the converted value on the stack.
fn load_offset(
    ctx: &mut FunctionContext<'_>,
    offset: Option<ValueId>,
    register: &str,
    spilled: bool,
) -> Result<()> {
    if spilled {
        abi::emit_pop_reg(ctx.emitter, register);
        return Ok(());
    }
    if let Some(offset) = offset {
        ctx.load_value_to_reg(offset, register).map(|_| ())
    } else {
        abi::emit_load_int_immediate(ctx.emitter, register, 0);
        Ok(())
    }
}

/// Returns a literal string operand defined by `ConstStr`.
fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Option<String>> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Ok(None);
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Ok(None);
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "unpack() string format operand has no data id",
        ));
    };
    Ok(Some(
        ctx.module
            .data
            .strings
            .get(data.as_raw() as usize)
            .cloned()
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?,
    ))
}
