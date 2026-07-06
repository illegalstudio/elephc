//! Purpose:
//! Shared argument marshalling for the `sprintf`/`printf` family. Pushes each value argument
//! as a 16-byte tagged record for `__rt_sprintf`, coercing the argument to the type its
//! conversion specifier consumes when the format string is a compile-time literal.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::sprintf::emit()`
//! - `crate::codegen_support::builtins::strings::printf::emit()`
//!
//! Key details:
//! - `__rt_sprintf` dispatches on the format specifier character (`f`/`e`/`g`→float, `s`→string,
//!   everything else→integer), NOT on the record tag. The int/float branches reinterpret the raw
//!   record payload, so the pushed payload must already match the specifier's type. This module
//!   parses literal formats to coerce each argument accordingly, fixing `Mixed`/cross-type args.
//! - The specifier scanner mirrors the runtime scanner exactly (flags → width → `.precision` →
//!   one type char), so spec boundaries and argument counts always agree with the runtime; for
//!   non-literal formats it falls back to the legacy push-by-static-type behavior.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::{coerce_result_to_type, emit_expr};
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// The value category a printf-family conversion specifier consumes, mirroring the
/// runtime's spec-character dispatch in `__rt_sprintf`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SpecCat {
    /// `%d`, `%i`, `%u`, `%x`, `%X`, `%o`, `%c`, and any other char the runtime int-branches.
    Int,
    /// `%f`, `%e`, `%g`.
    Float,
    /// `%s`.
    Str,
}

/// Parses a literal format string into the ordered value categories its specifiers consume.
///
/// The scan mirrors `__rt_sprintf` exactly: a `%` introduces a specifier unless followed by `%`
/// (a literal percent that consumes no argument); flags (`-`, `+`, `0`, space, `#`), width digits,
/// and an optional `.precision` run are skipped; the next byte is the type char. Classification
/// matches the runtime branch precisely — `f`/`e`/`g` are floats, `s` is a string, and every other
/// type char (including positional/length-modifier bytes the runtime mis-scans) is treated as the
/// runtime's default integer branch. Because the scanner agrees with the runtime byte-for-byte, the
/// returned categories align one-to-one with the arguments the runtime will read.
///
/// A specifier left incomplete at end-of-string is dropped, matching the runtime bailing out before
/// consuming an argument for it.
fn parse_format_spec_cats(fmt: &str) -> Vec<SpecCat> {
    let b = fmt.as_bytes();
    let mut cats = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1; // consume '%'
        if i >= b.len() {
            break; // lone trailing '%': runtime bails without consuming an argument
        }
        if b[i] == b'%' {
            i += 1; // "%%" is a literal percent, no argument consumed
            continue;
        }
        // flags
        while i < b.len() && matches!(b[i], b'-' | b'+' | b'0' | b' ' | b'#') {
            i += 1;
        }
        // width digits
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        // optional .precision
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i >= b.len() {
            break; // specifier ran off the end before a type char: runtime bails
        }
        let cat = match b[i] {
            b'f' | b'e' | b'g' => SpecCat::Float,
            b's' => SpecCat::Str,
            _ => SpecCat::Int,
        };
        cats.push(cat);
        i += 1; // consume the type char
    }
    cats
}

/// Emits the full sprintf-style marshalling sequence shared by `sprintf` and `printf`.
///
/// Each value argument is pushed as a 16-byte tagged record in reverse source order; when the
/// format is a literal, the argument is first coerced to the type its specifier consumes so the
/// runtime's spec-driven branch reads a correctly typed payload. The format string is then
/// evaluated and the argument count loaded before calling `__rt_sprintf`. On return the formatted
/// string occupies the standard string-result registers (`x1`/`x2` on ARM64, `rax`/`rdx` on x86_64)
/// and the runtime has popped the pushed records from the caller's stack.
pub(super) fn emit_format_and_call(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let arg_count = args.len() - 1; // exclude the format string

    // Determine per-argument target categories from a literal format. A non-literal format
    // yields no categories, so every argument falls back to push-by-static-type.
    let cats = match &args[0].kind {
        ExprKind::StringLiteral(s) => parse_format_spec_cats(s),
        _ => Vec::new(),
    };

    // -- evaluate and push arguments in reverse order --
    for i in (1..args.len()).rev() {
        let ty = emit_expr(&args[i], emitter, ctx, data);
        match cats.get(i - 1).copied() {
            Some(cat) => push_coerced(emitter, ctx, data, &ty, cat),
            None => push_static(emitter, &ty),
        }
    }

    // -- evaluate format string and pass the argument count --
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", arg_count));            // number of format arguments
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rdi", arg_count as i64);     // pass the number of packed variadic records in the first SysV integer argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_sprintf");                              // format the string through the target-aware sprintf runtime helper
    // runtime returns ptr+len and cleans up the caller's packed variadic records
}

/// Coerces the just-evaluated argument (result type `ty`) to the type its conversion specifier
/// consumes, then pushes a tagged record whose payload shape matches the runtime's branch for that
/// specifier. A statically-`Str` argument under a float specifier has no clean pointer/length →
/// double coercion, so it falls back to the legacy static push (a rare, pre-existing edge; `Mixed`
/// string-ish values still convert correctly via `__rt_mixed_cast_float`).
fn push_coerced(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    ty: &PhpType,
    cat: SpecCat,
) {
    if cat == SpecCat::Float && *ty == PhpType::Str {
        push_static(emitter, ty);
        return;
    }
    match cat {
        SpecCat::Int => {
            if *ty == PhpType::Str {
                // PHP string→int cast for a string arg under an integer specifier. Done locally
                // (rather than widening the shared `coerce_result_to_type` Str→Int contract, which
                // other call sites gate coercion on) so only this sprintf path is affected.
                abi::emit_call_label(emitter, "__rt_str_to_int");
            } else {
                coerce_result_to_type(emitter, ctx, data, ty, &PhpType::Int);
            }
            push_int(emitter);
        }
        SpecCat::Float => {
            coerce_result_to_type(emitter, ctx, data, ty, &PhpType::Float);
            push_float(emitter);
        }
        SpecCat::Str => {
            coerce_result_to_type(emitter, ctx, data, ty, &PhpType::Str);
            push_str(emitter);
        }
    }
}

/// Pushes a tagged record for an argument using its static type without coercion, matching the
/// historical behavior used for non-literal formats and for arguments with no matching specifier.
fn push_static(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Int => push_int(emitter),
        PhpType::Float => push_float(emitter),
        PhpType::Bool => push_bool(emitter),
        PhpType::Str => push_str(emitter),
        _ => push_zero(emitter),
    }
}

/// Pushes the integer in the integer-result register as a tag-0 record (payload in the low qword).
fn push_int(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // push the integer value as the record payload
            emitter.instruction("str xzr, [sp, #8]");                           // type tag 0 = int
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one 16-byte tagged argument record
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // store the integer payload in the low half of the record
            emitter.instruction("mov QWORD PTR [rsp + 8], 0");                  // tag the record as an integer operand
        }
    }
}

/// Pushes the float bits in the float-result register as a tag-2 record (bits in the low qword).
fn push_float(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov x0, d0");                                 // move float bits into an integer register for the record payload
            emitter.instruction("str x0, [sp, #-16]!");                         // push the float bits as the record payload
            emitter.instruction("mov x0, #2");                                  // type tag 2 = float
            emitter.instruction("str x0, [sp, #8]");                            // store the type tag
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one 16-byte tagged argument record
            emitter.instruction("movsd QWORD PTR [rsp], xmm0");                 // store the float bits in the low half of the record
            emitter.instruction("mov QWORD PTR [rsp + 8], 2");                  // tag the record as a floating operand
        }
    }
}

/// Pushes the boolean in the integer-result register as a tag-3 record.
fn push_bool(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // push the boolean value (0 or 1) as the record payload
            emitter.instruction("mov x0, #3");                                  // type tag 3 = bool
            emitter.instruction("str x0, [sp, #8]");                            // store the type tag
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one 16-byte tagged argument record
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // store the boolean payload in the low half of the record
            emitter.instruction("mov QWORD PTR [rsp + 8], 3");                  // tag the record as a boolean operand
        }
    }
}

/// Pushes the string in the string-result registers as a tag-1 record (pointer in the low qword,
/// `length << 8 | 1` in the high qword so the runtime str branch can recover the length).
fn push_str(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x1, [sp, #-16]!");                         // push the string pointer as the record payload
            emitter.instruction("lsl x0, x2, #8");                              // shift the length left by 8 to make room for the tag bit
            emitter.instruction("orr x0, x0, #1");                              // set type tag bit 0 = str
            emitter.instruction("str x0, [sp, #8]");                            // store tag|length
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one 16-byte tagged argument record
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // store the string pointer in the low half of the record
            emitter.instruction("mov rcx, rdx");                                // copy the length before packing it into the metadata word
            emitter.instruction("shl rcx, 8");                                  // shift the length into the upper metadata bits
            emitter.instruction("or rcx, 1");                                   // set type tag bit 0 = str while preserving the packed length
            emitter.instruction("mov QWORD PTR [rsp + 8], rcx");                // store the packed string metadata word
        }
    }
}

/// Pushes a zero-valued tag-0 record for argument types that have no printf payload representation.
fn push_zero(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str xzr, [sp, #-16]!");                        // push a zero payload for an unsupported operand
            emitter.instruction("str xzr, [sp, #8]");                           // type tag 0
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve one 16-byte tagged argument record
            emitter.instruction("mov QWORD PTR [rsp], 0");                      // store a zero payload for an unsupported operand
            emitter.instruction("mov QWORD PTR [rsp + 8], 0");                  // tag the unsupported operand as an integer zero fallback
        }
    }
}
