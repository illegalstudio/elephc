//! Purpose:
//! Lowers PHP `ctype_*` character-class builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` for the typed `RuntimeFnId::Ctype*` targets.
//!
//! Key details:
//! - PHP ctype checks operate over bytes in the C locale, reject empty strings, and return
//!   scalar booleans.
//! - Non-string arguments (ints, boxed `mixed`, and every other type) are handled in
//!   `values`, which emits PHP's deprecation and reuses the byte predicates below.
//! - The loops are emitted inline; there is no shared runtime helper.

mod values;

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::{expect_operand, store_if_result};

/// Lowers `ctype_alpha(string)` by checking every byte against ASCII alpha ranges.
pub(crate) fn lower_ctype_alpha(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Alpha)
}

/// Lowers `ctype_digit(string)` by checking every byte against the ASCII digit range.
pub(crate) fn lower_ctype_digit(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Digit)
}

/// Lowers `ctype_alnum(string)` by checking every byte against ASCII alpha or digit ranges.
pub(crate) fn lower_ctype_alnum(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Alnum)
}

/// Lowers `ctype_space(string)` by checking every byte against PHP's ASCII whitespace set.
pub(crate) fn lower_ctype_space(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Space)
}

/// Lowers `ctype_cntrl(string)` by checking every byte against ASCII controls, including DEL.
pub(crate) fn lower_ctype_cntrl(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Cntrl)
}

/// Lowers `ctype_graph(string)` by checking every byte against visible ASCII, excluding space.
pub(crate) fn lower_ctype_graph(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Graph)
}

/// Lowers `ctype_lower(string)` by checking every byte against the ASCII lowercase range.
pub(crate) fn lower_ctype_lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Lower)
}

/// Lowers `ctype_print(string)` by checking every byte against printable ASCII, including space.
pub(crate) fn lower_ctype_print(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Print)
}

/// Lowers `ctype_punct(string)` by checking every byte against ASCII punctuation.
pub(crate) fn lower_ctype_punct(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Punct)
}

/// Lowers `ctype_upper(string)` by checking every byte against the ASCII uppercase range.
pub(crate) fn lower_ctype_upper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Upper)
}

/// Lowers `ctype_xdigit(string)` by checking every byte against ASCII hexadecimal digits.
pub(crate) fn lower_ctype_xdigit(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Xdigit)
}

/// Identifies the byte predicate implemented by a ctype builtin.
#[derive(Clone, Copy)]
enum CtypeKind {
    Alpha,
    Digit,
    Alnum,
    Space,
    Cntrl,
    Graph,
    Lower,
    Print,
    Punct,
    Upper,
    Xdigit,
}

impl CtypeKind {
    /// Returns the PHP-visible builtin name for diagnostics and comments.
    fn name(self) -> &'static str {
        match self {
            CtypeKind::Alpha => "ctype_alpha",
            CtypeKind::Digit => "ctype_digit",
            CtypeKind::Alnum => "ctype_alnum",
            CtypeKind::Space => "ctype_space",
            CtypeKind::Cntrl => "ctype_cntrl",
            CtypeKind::Graph => "ctype_graph",
            CtypeKind::Lower => "ctype_lower",
            CtypeKind::Print => "ctype_print",
            CtypeKind::Punct => "ctype_punct",
            CtypeKind::Upper => "ctype_upper",
            CtypeKind::Xdigit => "ctype_xdigit",
        }
    }

    /// Returns PHP's answer for a non-negative integer above 255, whose decimal digits form
    /// the string php-src would classify.
    fn accepts_digit_string(self) -> bool {
        matches!(
            self,
            CtypeKind::Alnum | CtypeKind::Digit | CtypeKind::Graph | CtypeKind::Print | CtypeKind::Xdigit
        )
    }

    /// Returns PHP's answer for an integer below -128, whose decimal form starts with `-`.
    fn accepts_negative_digit_string(self) -> bool {
        matches!(self, CtypeKind::Graph | CtypeKind::Print)
    }
}

/// Labels shared by the string scan, the integer classification, and the boolean result tail.
struct CtypeLabels {
    /// First instruction of the string scan, reached with the string in the result registers.
    string_entry: String,
    loop_label: String,
    next_label: String,
    fail_label: String,
    pass_label: String,
    end_label: String,
}

impl CtypeLabels {
    /// Allocates one fresh label set for a single `ctype_*` call.
    fn new(ctx: &mut FunctionContext<'_>) -> Self {
        Self {
            string_entry: ctx.next_label("ctype_string"),
            loop_label: ctx.next_label("ctype_loop"),
            next_label: ctx.next_label("ctype_next"),
            fail_label: ctx.next_label("ctype_fail"),
            pass_label: ctx.next_label("ctype_pass"),
            end_label: ctx.next_label("ctype_end"),
        }
    }
}

/// Lowers one `ctype_*` call over its `mixed` argument and stores the boolean result.
///
/// A string is scanned byte by byte. Every other argument follows PHP 8.1+: a deprecation,
/// then the integer single-byte rules or a plain `false` (see `values`).
fn lower_ctype(ctx: &mut FunctionContext<'_>, inst: &Instruction, kind: CtypeKind) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            kind.name(),
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    let labels = CtypeLabels::new(ctx);
    ctx.emitter.comment(&format!("{}()", kind.name()));
    let ty = ctx.value_php_type(value)?;
    let needs_string_scan = if ty.codegen_repr() == PhpType::Str {
        ctx.load_value_to_result(value)?;
        true
    } else {
        values::emit_non_string_argument(ctx, kind, value, &ty, &labels)?
    };
    if needs_string_scan {
        match ctx.emitter.target.arch {
            Arch::AArch64 => emit_aarch64_string_scan(ctx, kind, &labels),
            Arch::X86_64 => emit_x86_64_string_scan(ctx, kind, &labels),
        }
    }
    emit_result_tail(ctx, &labels);
    store_if_result(ctx, inst)
}

/// Emits the `false`/`true` materialization every ctype path branches to.
fn emit_result_tail(ctx: &mut FunctionContext<'_>, labels: &CtypeLabels) {
    let result = abi::int_result_reg(ctx.emitter);
    ctx.emitter.label(&labels.fail_label);
    abi::emit_load_int_immediate(ctx.emitter, result, 0);
    abi::emit_jump(ctx.emitter, &labels.end_label);
    ctx.emitter.label(&labels.pass_label);
    abi::emit_load_int_immediate(ctx.emitter, result, 1);
    ctx.emitter.label(&labels.end_label);
}

/// Emits the AArch64 string scan over the payload in `x1`/`x2`.
fn emit_aarch64_string_scan(ctx: &mut FunctionContext<'_>, kind: CtypeKind, labels: &CtypeLabels) {
    ctx.emitter.label(&labels.string_entry);
    // -- scan every byte of the string payload --
    ctx.emitter.instruction(&format!("cbz x2, {}", labels.fail_label));         // empty strings fail PHP ctype predicates
    ctx.emitter.instruction("mov x3, #0");                                      // start scanning at byte offset zero
    ctx.emitter.label(&labels.loop_label);
    ctx.emitter.instruction("cmp x3, x2");                                      // check whether every byte has been examined
    ctx.emitter.instruction(&format!("b.ge {}", labels.pass_label));            // report success once the scan reaches the string length
    ctx.emitter.instruction("ldrb w4, [x1, x3]");                               // load the current byte from the PHP string payload
    emit_aarch64_ctype_predicate(ctx, kind, &labels.next_label, &labels.fail_label);
    ctx.emitter.label(&labels.next_label);
    ctx.emitter.instruction("add x3, x3, #1");                                  // advance to the next byte after accepting the current byte
    ctx.emitter.instruction(&format!("b {}", labels.loop_label));               // continue scanning the remaining bytes
}

/// Emits the AArch64 byte predicate for one ctype family.
fn emit_aarch64_ctype_predicate(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    next_label: &str,
    fail_label: &str,
) {
    match kind {
        CtypeKind::Alpha => {
            emit_aarch64_uppercase_accept(ctx, next_label);
            emit_aarch64_lowercase_reject(ctx, fail_label);
        }
        CtypeKind::Digit => emit_aarch64_digit_reject(ctx, fail_label),
        CtypeKind::Alnum => {
            emit_aarch64_uppercase_accept(ctx, next_label);
            emit_aarch64_lowercase_accept(ctx, next_label);
            emit_aarch64_digit_reject(ctx, fail_label);
        }
        CtypeKind::Space => emit_aarch64_space_reject(ctx, next_label, fail_label),
        CtypeKind::Cntrl => emit_aarch64_cntrl_reject(ctx, next_label, fail_label),
        CtypeKind::Graph => emit_aarch64_span_reject(ctx, 33, 93, fail_label),
        CtypeKind::Lower => emit_aarch64_span_reject(ctx, 97, 25, fail_label),
        CtypeKind::Print => emit_aarch64_span_reject(ctx, 32, 94, fail_label),
        CtypeKind::Punct => {
            emit_aarch64_span_accept(ctx, 33, 14, next_label);
            emit_aarch64_span_accept(ctx, 58, 6, next_label);
            emit_aarch64_span_accept(ctx, 91, 5, next_label);
            emit_aarch64_span_reject(ctx, 123, 3, fail_label);
        }
        CtypeKind::Upper => emit_aarch64_span_reject(ctx, 65, 25, fail_label),
        CtypeKind::Xdigit => {
            emit_aarch64_span_accept(ctx, 48, 9, next_label);
            emit_aarch64_span_accept(ctx, 65, 5, next_label);
            emit_aarch64_span_reject(ctx, 97, 5, fail_label);
        }
    }
}

/// Emits an AArch64 uppercase ASCII range probe that branches on success.
fn emit_aarch64_uppercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    // -- accept an uppercase ASCII byte --
    ctx.emitter.instruction("sub w5, w4, #65");                                 // normalize the byte against uppercase ASCII 'A'
    ctx.emitter.instruction("cmp w5, #25");                                     // check whether the byte is inside A-Z
    ctx.emitter.instruction(&format!("b.ls {}", next_label));                   // accept uppercase ASCII letters
}

/// Emits an AArch64 lowercase ASCII range probe that branches on success.
fn emit_aarch64_lowercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    // -- accept a lowercase ASCII byte --
    ctx.emitter.instruction("sub w5, w4, #97");                                 // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp w5, #25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("b.ls {}", next_label));                   // accept lowercase ASCII letters
}

/// Emits an AArch64 lowercase ASCII range probe that branches on failure.
fn emit_aarch64_lowercase_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
    // -- reject a byte outside lowercase ASCII --
    ctx.emitter.instruction("sub w5, w4, #97");                                 // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp w5, #25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("b.hi {}", fail_label));                   // reject bytes outside lowercase ASCII after uppercase failed
}

/// Emits an AArch64 digit ASCII range probe that branches on failure.
fn emit_aarch64_digit_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
    // -- reject a byte outside the ASCII digits --
    ctx.emitter.instruction("sub w5, w4, #48");                                 // normalize the byte against ASCII '0'
    ctx.emitter.instruction("cmp w5, #9");                                      // check whether the byte is inside 0-9
    ctx.emitter.instruction(&format!("b.hi {}", fail_label));                   // reject bytes outside the ASCII digit range
}

/// Emits an AArch64 whitespace probe that branches on accepted bytes or final failure.
fn emit_aarch64_space_reject(
    ctx: &mut FunctionContext<'_>,
    next_label: &str,
    fail_label: &str,
) {
    // -- accept PHP's ASCII whitespace set --
    ctx.emitter.instruction("cmp w4, #32");                                     // check for ASCII space
    ctx.emitter.instruction(&format!("b.eq {}", next_label));                   // accept ASCII space
    ctx.emitter.instruction("cmp w4, #9");                                      // check for horizontal tab
    ctx.emitter.instruction(&format!("b.eq {}", next_label));                   // accept horizontal tab
    ctx.emitter.instruction("cmp w4, #10");                                     // check for newline
    ctx.emitter.instruction(&format!("b.eq {}", next_label));                   // accept newline
    ctx.emitter.instruction("cmp w4, #13");                                     // check for carriage return
    ctx.emitter.instruction(&format!("b.eq {}", next_label));                   // accept carriage return
    ctx.emitter.instruction("cmp w4, #11");                                     // check for vertical tab
    ctx.emitter.instruction(&format!("b.eq {}", next_label));                   // accept vertical tab
    ctx.emitter.instruction("cmp w4, #12");                                     // check for form feed
    ctx.emitter.instruction(&format!("b.ne {}", fail_label));                   // reject bytes outside PHP's ASCII whitespace set
}

/// Emits the x86_64 string scan over the payload in `rax`/`rdx`.
fn emit_x86_64_string_scan(ctx: &mut FunctionContext<'_>, kind: CtypeKind, labels: &CtypeLabels) {
    ctx.emitter.label(&labels.string_entry);
    // -- scan every byte of the string payload --
    ctx.emitter.instruction("test rdx, rdx");                                   // empty strings fail PHP ctype predicates
    ctx.emitter.instruction(&format!("je {}", labels.fail_label));              // branch to false for an empty input string
    ctx.emitter.instruction("xor rcx, rcx");                                    // start scanning at byte offset zero
    ctx.emitter.label(&labels.loop_label);
    ctx.emitter.instruction("cmp rcx, rdx");                                    // check whether every byte has been examined
    ctx.emitter.instruction(&format!("jge {}", labels.pass_label));             // report success once the scan reaches the string length
    ctx.emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");                 // load the current byte from the PHP string payload
    emit_x86_64_ctype_predicate(ctx, kind, &labels.next_label, &labels.fail_label);
    ctx.emitter.label(&labels.next_label);
    ctx.emitter.instruction("add rcx, 1");                                      // advance to the next byte after accepting the current byte
    ctx.emitter.instruction(&format!("jmp {}", labels.loop_label));             // continue scanning the remaining bytes
}

/// Emits the x86_64 byte predicate for one ctype family.
fn emit_x86_64_ctype_predicate(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    next_label: &str,
    fail_label: &str,
) {
    match kind {
        CtypeKind::Alpha => {
            emit_x86_64_uppercase_accept(ctx, next_label);
            emit_x86_64_lowercase_reject(ctx, fail_label);
        }
        CtypeKind::Digit => emit_x86_64_digit_reject(ctx, fail_label),
        CtypeKind::Alnum => {
            emit_x86_64_uppercase_accept(ctx, next_label);
            emit_x86_64_lowercase_accept(ctx, next_label);
            emit_x86_64_digit_reject(ctx, fail_label);
        }
        CtypeKind::Space => emit_x86_64_space_reject(ctx, next_label, fail_label),
        CtypeKind::Cntrl => emit_x86_64_cntrl_reject(ctx, next_label, fail_label),
        CtypeKind::Graph => emit_x86_64_span_reject(ctx, 33, 93, fail_label),
        CtypeKind::Lower => emit_x86_64_span_reject(ctx, 97, 25, fail_label),
        CtypeKind::Print => emit_x86_64_span_reject(ctx, 32, 94, fail_label),
        CtypeKind::Punct => {
            emit_x86_64_span_accept(ctx, 33, 14, next_label);
            emit_x86_64_span_accept(ctx, 58, 6, next_label);
            emit_x86_64_span_accept(ctx, 91, 5, next_label);
            emit_x86_64_span_reject(ctx, 123, 3, fail_label);
        }
        CtypeKind::Upper => emit_x86_64_span_reject(ctx, 65, 25, fail_label),
        CtypeKind::Xdigit => {
            emit_x86_64_span_accept(ctx, 48, 9, next_label);
            emit_x86_64_span_accept(ctx, 65, 5, next_label);
            emit_x86_64_span_reject(ctx, 97, 5, fail_label);
        }
    }
}

/// Emits an x86_64 uppercase ASCII range probe that branches on success.
fn emit_x86_64_uppercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    // -- accept an uppercase ASCII byte --
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before normalizing against uppercase ASCII
    ctx.emitter.instruction("sub r9d, 65");                                     // normalize the byte against uppercase ASCII 'A'
    ctx.emitter.instruction("cmp r9d, 25");                                     // check whether the byte is inside A-Z
    ctx.emitter.instruction(&format!("jbe {}", next_label));                    // accept uppercase ASCII letters
}

/// Emits an x86_64 lowercase ASCII range probe that branches on success.
fn emit_x86_64_lowercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    // -- accept a lowercase ASCII byte --
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before normalizing against lowercase ASCII
    ctx.emitter.instruction("sub r9d, 97");                                     // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp r9d, 25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("jbe {}", next_label));                    // accept lowercase ASCII letters
}

/// Emits an x86_64 lowercase ASCII range probe that branches on failure.
fn emit_x86_64_lowercase_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
    // -- reject a byte outside lowercase ASCII --
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before normalizing against lowercase ASCII
    ctx.emitter.instruction("sub r9d, 97");                                     // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp r9d, 25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("ja {}", fail_label));                     // reject bytes outside lowercase ASCII after uppercase failed
}

/// Emits an x86_64 digit ASCII range probe that branches on failure.
fn emit_x86_64_digit_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
    // -- reject a byte outside the ASCII digits --
    ctx.emitter.instruction("sub r8d, 48");                                     // normalize the byte against ASCII '0'
    ctx.emitter.instruction("cmp r8d, 9");                                      // check whether the byte is inside 0-9
    ctx.emitter.instruction(&format!("ja {}", fail_label));                     // reject bytes outside the ASCII digit range
}

/// Emits an x86_64 whitespace probe that branches on accepted bytes or final failure.
fn emit_x86_64_space_reject(
    ctx: &mut FunctionContext<'_>,
    next_label: &str,
    fail_label: &str,
) {
    // -- accept PHP's ASCII whitespace set --
    ctx.emitter.instruction("cmp r8d, 32");                                     // check for ASCII space
    ctx.emitter.instruction(&format!("je {}", next_label));                     // accept ASCII space
    ctx.emitter.instruction("cmp r8d, 9");                                      // check for horizontal tab
    ctx.emitter.instruction(&format!("je {}", next_label));                     // accept horizontal tab
    ctx.emitter.instruction("cmp r8d, 10");                                     // check for newline
    ctx.emitter.instruction(&format!("je {}", next_label));                     // accept newline
    ctx.emitter.instruction("cmp r8d, 13");                                     // check for carriage return
    ctx.emitter.instruction(&format!("je {}", next_label));                     // accept carriage return
    ctx.emitter.instruction("cmp r8d, 11");                                     // check for vertical tab
    ctx.emitter.instruction(&format!("je {}", next_label));                     // accept vertical tab
    ctx.emitter.instruction("cmp r8d, 12");                                     // check for form feed
    ctx.emitter.instruction(&format!("jne {}", fail_label));                    // reject bytes outside PHP's ASCII whitespace set
}

/// Emits an AArch64 inclusive ASCII span check that branches to `next_label` on a hit.
///
/// `base` is the first accepted byte. `end_offset` is the unsigned distance from `base`
/// to the last accepted byte.
fn emit_aarch64_span_accept(
    ctx: &mut FunctionContext<'_>,
    base: u32,
    end_offset: u32,
    next_label: &str,
) {
    // -- accept a byte inside one ASCII span --
    ctx.emitter.instruction(&format!("sub w5, w4, #{base}"));                   // shift the byte so the ASCII span starts at zero
    ctx.emitter.instruction(&format!("cmp w5, #{end_offset}"));                 // compare against the last byte of the span
    ctx.emitter.instruction(&format!("b.ls {}", next_label));                   // accept a byte inside this ASCII span
}

/// Emits an AArch64 inclusive ASCII span check that branches to `fail_label` on a miss.
fn emit_aarch64_span_reject(
    ctx: &mut FunctionContext<'_>,
    base: u32,
    end_offset: u32,
    fail_label: &str,
) {
    // -- reject a byte outside one ASCII span --
    ctx.emitter.instruction(&format!("sub w5, w4, #{base}"));                   // shift the byte so the ASCII span starts at zero
    ctx.emitter.instruction(&format!("cmp w5, #{end_offset}"));                 // compare against the last byte of the span
    ctx.emitter.instruction(&format!("b.hi {}", fail_label));                   // reject a byte outside this ASCII span
}

/// Emits the AArch64 control-character probe, including ASCII DEL.
fn emit_aarch64_cntrl_reject(
    ctx: &mut FunctionContext<'_>,
    next_label: &str,
    fail_label: &str,
) {
    // -- accept C0 controls and DEL, reject every other byte --
    ctx.emitter.instruction("cmp w4, #31");                                     // check the ASCII control range 0x00-0x1F
    ctx.emitter.instruction(&format!("b.ls {}", next_label));                   // accept C0 control bytes
    ctx.emitter.instruction("cmp w4, #127");                                    // check for ASCII DEL
    ctx.emitter.instruction(&format!("b.ne {}", fail_label));                   // reject every other byte, including bytes above 0x7F
}

/// Emits an x86_64 inclusive ASCII span check that branches to `next_label` on a hit.
///
/// The original byte in `r8d` stays intact so a later span can test the same byte.
fn emit_x86_64_span_accept(
    ctx: &mut FunctionContext<'_>,
    base: u32,
    end_offset: u32,
    next_label: &str,
) {
    // -- accept a byte inside one ASCII span --
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before the range normalization
    ctx.emitter.instruction(&format!("sub r9d, {base}"));                       // shift the byte so the ASCII span starts at zero
    ctx.emitter.instruction(&format!("cmp r9d, {end_offset}"));                 // compare against the last byte of the span
    ctx.emitter.instruction(&format!("jbe {}", next_label));                    // accept a byte inside this ASCII span
}

/// Emits an x86_64 inclusive ASCII span check that branches to `fail_label` on a miss.
fn emit_x86_64_span_reject(
    ctx: &mut FunctionContext<'_>,
    base: u32,
    end_offset: u32,
    fail_label: &str,
) {
    // -- reject a byte outside one ASCII span --
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before the range normalization
    ctx.emitter.instruction(&format!("sub r9d, {base}"));                       // shift the byte so the ASCII span starts at zero
    ctx.emitter.instruction(&format!("cmp r9d, {end_offset}"));                 // compare against the last byte of the span
    ctx.emitter.instruction(&format!("ja {}", fail_label));                     // reject a byte outside this ASCII span
}

/// Emits the x86_64 control-character probe, including ASCII DEL.
fn emit_x86_64_cntrl_reject(
    ctx: &mut FunctionContext<'_>,
    next_label: &str,
    fail_label: &str,
) {
    // -- accept C0 controls and DEL, reject every other byte --
    ctx.emitter.instruction("cmp r8d, 31");                                     // check the ASCII control range 0x00-0x1F
    ctx.emitter.instruction(&format!("jbe {}", next_label));                    // accept C0 control bytes
    ctx.emitter.instruction("cmp r8d, 127");                                    // check for ASCII DEL
    ctx.emitter.instruction(&format!("jne {}", fail_label));                    // reject every other byte, including bytes above 0x7F
}
