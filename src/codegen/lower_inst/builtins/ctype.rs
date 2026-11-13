//! Purpose:
//! Lowers PHP `ctype_*` character-class builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - PHP ctype checks operate over bytes, reject empty strings, and return scalar booleans.
//! - The loops are emitted inline; there is no shared runtime helper.
//! - The argument is `mixed`, as in php-src: an int in -128..=255 is one character code, any
//!   other int is checked as its decimal string, and every other non-string type is false.

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

/// Identifies the byte predicate implemented by a ctype builtin.
#[derive(Clone, Copy)]
enum CtypeKind {
    Alpha,
    Digit,
    Alnum,
    Space,
}

impl CtypeKind {
    /// Returns the PHP-visible builtin name for diagnostics and comments.
    fn name(self) -> &'static str {
        match self {
            CtypeKind::Alpha => "ctype_alpha",
            CtypeKind::Digit => "ctype_digit",
            CtypeKind::Alnum => "ctype_alnum",
            CtypeKind::Space => "ctype_space",
        }
    }
}

/// Emits the shared ctype byte-scanning loop and stores the boolean result.
///
/// The operand keeps its own type (the contract takes `mixed` and the argument lowering
/// preserves values), so the subject is chosen here: a string is scanned as is, an int goes
/// through [`emit_int_subject`], a boxed value is unboxed and dispatched on its runtime tag, and
/// any other static type is false without a scan. Coercing every argument to a string first made
/// `ctype_digit(7)` true, where php checks character 7 and answers false.
fn lower_ctype(ctx: &mut FunctionContext<'_>, inst: &Instruction, kind: CtypeKind) -> Result<()> {
    let operand_ty = load_single_arg(ctx, inst, kind.name())?;
    let loop_label = ctx.next_label("ctype_loop");
    let next_label = ctx.next_label("ctype_next");
    let fail_label = ctx.next_label("ctype_fail");
    let pass_label = ctx.next_label("ctype_pass");
    let end_label = ctx.next_label("ctype_end");
    let scan_label = ctx.next_label("ctype_scan");
    let int_label = ctx.next_label("ctype_int");
    match operand_ty {
        PhpType::Str => {}
        PhpType::Int => {
            emit_int_subject(ctx, kind, &scan_label, &fail_label, &pass_label);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_boxed_subject_dispatch(ctx, &scan_label, &int_label, &fail_label);
            ctx.emitter.label(&int_label);
            emit_int_subject(ctx, kind, &scan_label, &fail_label, &pass_label);
        }
        _ => {
            // Bool, float, null, arrays and objects are never a ctype subject in php.
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            return store_if_result(ctx, inst);
        }
    }
    ctx.emitter.label(&scan_label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_aarch64_ctype_loop(
            ctx,
            kind,
            &loop_label,
            &next_label,
            &fail_label,
            &pass_label,
            &end_label,
        ),
        Arch::X86_64 => emit_x86_64_ctype_loop(
            ctx,
            kind,
            &loop_label,
            &next_label,
            &fail_label,
            &pass_label,
            &end_label,
        ),
    }
    ctx.emitter.label(&end_label);
    store_if_result(ctx, inst)
}

/// Loads the single ctype argument into the result registers and returns its codegen type.
///
/// A string lands in the string result pair the scan loop reads, an int in the integer result
/// register, and a boxed value as its cell pointer for [`emit_boxed_subject_dispatch`].
fn load_single_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<PhpType> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "{} expected 1 arg, got {}",
            name,
            inst.operands.len()
        )));
    }
    let value = expect_operand(inst, 0)?;
    Ok(ctx.load_value_to_result(value)?.codegen_repr())
}

/// Unboxes the loaded Mixed cell and routes it by runtime tag: a string to the scan with its
/// payload in the string result pair, an int to `int_label` with it in the integer result
/// register, anything else to `fail_label`.
fn emit_boxed_subject_dispatch(
    ctx: &mut FunctionContext<'_>,
    scan_label: &str,
    int_label: &str,
    fail_label: &str,
) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #1");                              // a string payload is already in the x1/x2 pair the scan reads
            ctx.emitter.instruction(&format!("b.eq {}", scan_label));           // scan a boxed string byte by byte
            ctx.emitter.instruction(&format!("cbnz x0, {}", fail_label));       // every tag but int and string answers false
            ctx.emitter.instruction("mov x0, x1");                              // move the boxed int into the integer result register
            ctx.emitter.instruction(&format!("b {}", int_label));               // classify the int as a character code or its digits
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 1");                              // a string payload is in rdi/rdx after unboxing
            ctx.emitter.instruction(&format!("jne {}_not_string", scan_label)); // route non-strings to the int test
            ctx.emitter.instruction("mov rax, rdi");                            // move the string pointer into the scan's rax/rdx pair
            ctx.emitter.instruction(&format!("jmp {}", scan_label));            // scan a boxed string byte by byte
            ctx.emitter.label(&format!("{}_not_string", scan_label));
            ctx.emitter.instruction("test rax, rax");                           // tag zero is a PHP int
            ctx.emitter.instruction(&format!("jne {}", fail_label));            // every tag but int and string answers false
            ctx.emitter.instruction("mov rax, rdi");                            // move the boxed int into the integer result register
            ctx.emitter.instruction(&format!("jmp {}", int_label));             // classify the int as a character code or its digits
        }
    }
}

/// Classifies the int in the integer result register the way php-src does.
///
/// -128..=255 is ONE character code (a negative value wraps by 256, which is its low byte), so
/// its single byte is tested and the result is final. Any other int is formatted with
/// `__rt_itoa`, which returns the digits in the string result pair, and falls into the scan.
fn emit_int_subject(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    scan_label: &str,
    fail_label: &str,
    pass_label: &str,
) {
    let digits_label = ctx.next_label("ctype_int_digits");
    let byte_pass_label = ctx.next_label("ctype_byte_pass");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("add x9, x0, #128");                        // map the character-code range -128..255 onto 0..383
            ctx.emitter.instruction("cmp x9, #383");                            // an unsigned compare also rejects everything below -128
            ctx.emitter.instruction(&format!("b.hi {}", digits_label));         // larger magnitudes are checked as decimal strings
            ctx.emitter.instruction("and w4, w0, #255");                        // the character code is the low byte
            emit_aarch64_ctype_predicate(ctx, kind, &byte_pass_label, fail_label);
            ctx.emitter.label(&byte_pass_label);
            ctx.emitter.instruction(&format!("b {}", pass_label));              // the one character satisfied the class
            ctx.emitter.label(&digits_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea r9, [rax + 128]");                     // map the character-code range -128..255 onto 0..383
            ctx.emitter.instruction("cmp r9, 383");                             // an unsigned compare also rejects everything below -128
            ctx.emitter.instruction(&format!("ja {}", digits_label));           // larger magnitudes are checked as decimal strings
            ctx.emitter.instruction("movzx r8d, al");                           // the character code is the low byte
            emit_x86_64_ctype_predicate(ctx, kind, &byte_pass_label, fail_label);
            ctx.emitter.label(&byte_pass_label);
            ctx.emitter.instruction(&format!("jmp {}", pass_label));            // the one character satisfied the class
            ctx.emitter.label(&digits_label);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_itoa");                             // the decimal digits land in the string result pair
    abi::emit_jump(ctx.emitter, scan_label);
}

/// Emits the AArch64 ctype scan loop.
fn emit_aarch64_ctype_loop(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    loop_label: &str,
    next_label: &str,
    fail_label: &str,
    pass_label: &str,
    end_label: &str,
) {
    ctx.emitter.comment(&format!("{}()", kind.name()));
    ctx.emitter.instruction(&format!("cbz x2, {}", fail_label));                // empty strings fail PHP ctype predicates
    ctx.emitter.instruction("mov x3, #0");                                      // start scanning at byte offset zero
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp x3, x2");                                      // check whether every byte has been examined
    ctx.emitter.instruction(&format!("b.ge {}", pass_label));                   // report success once the scan reaches the string length
    ctx.emitter.instruction("ldrb w4, [x1, x3]");                               // load the current byte from the PHP string payload
    emit_aarch64_ctype_predicate(ctx, kind, next_label, fail_label);
    ctx.emitter.label(next_label);
    ctx.emitter.instruction("add x3, x3, #1");                                  // advance to the next byte after accepting the current byte
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // continue scanning the remaining bytes
    ctx.emitter.label(fail_label);
    ctx.emitter.instruction("mov x0, #0");                                      // materialize false for an empty string or rejected byte
    ctx.emitter.instruction(&format!("b {}", end_label));                       // skip the success result after a failed predicate
    ctx.emitter.label(pass_label);
    ctx.emitter.instruction("mov x0, #1");                                      // materialize true when every byte satisfies the predicate
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
    }
}

/// Emits an AArch64 uppercase ASCII range probe that branches on success.
fn emit_aarch64_uppercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    ctx.emitter.instruction("sub w5, w4, #65");                                 // normalize the byte against uppercase ASCII 'A'
    ctx.emitter.instruction("cmp w5, #25");                                     // check whether the byte is inside A-Z
    ctx.emitter.instruction(&format!("b.ls {}", next_label));                   // accept uppercase ASCII letters
}

/// Emits an AArch64 lowercase ASCII range probe that branches on success.
fn emit_aarch64_lowercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    ctx.emitter.instruction("sub w5, w4, #97");                                 // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp w5, #25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("b.ls {}", next_label));                   // accept lowercase ASCII letters
}

/// Emits an AArch64 lowercase ASCII range probe that branches on failure.
fn emit_aarch64_lowercase_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
    ctx.emitter.instruction("sub w5, w4, #97");                                 // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp w5, #25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("b.hi {}", fail_label));                   // reject bytes outside lowercase ASCII after uppercase failed
}

/// Emits an AArch64 digit ASCII range probe that branches on failure.
fn emit_aarch64_digit_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
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

/// Emits the x86_64 ctype scan loop.
fn emit_x86_64_ctype_loop(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    loop_label: &str,
    next_label: &str,
    fail_label: &str,
    pass_label: &str,
    end_label: &str,
) {
    ctx.emitter.comment(&format!("{}()", kind.name()));
    ctx.emitter.instruction("test rdx, rdx");                                   // empty strings fail PHP ctype predicates
    ctx.emitter.instruction(&format!("je {}", fail_label));                     // branch to false for an empty input string
    ctx.emitter.instruction("xor rcx, rcx");                                    // start scanning at byte offset zero
    ctx.emitter.label(loop_label);
    ctx.emitter.instruction("cmp rcx, rdx");                                    // check whether every byte has been examined
    ctx.emitter.instruction(&format!("jge {}", pass_label));                    // report success once the scan reaches the string length
    ctx.emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");                 // load the current byte from the PHP string payload
    emit_x86_64_ctype_predicate(ctx, kind, next_label, fail_label);
    ctx.emitter.label(next_label);
    ctx.emitter.instruction("add rcx, 1");                                      // advance to the next byte after accepting the current byte
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // continue scanning the remaining bytes
    ctx.emitter.label(fail_label);
    ctx.emitter.instruction("mov rax, 0");                                      // materialize false for an empty string or rejected byte
    ctx.emitter.instruction(&format!("jmp {}", end_label));                     // skip the success result after a failed predicate
    ctx.emitter.label(pass_label);
    ctx.emitter.instruction("mov rax, 1");                                      // materialize true when every byte satisfies the predicate
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
    }
}

/// Emits an x86_64 uppercase ASCII range probe that branches on success.
fn emit_x86_64_uppercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before normalizing against uppercase ASCII
    ctx.emitter.instruction("sub r9d, 65");                                     // normalize the byte against uppercase ASCII 'A'
    ctx.emitter.instruction("cmp r9d, 25");                                     // check whether the byte is inside A-Z
    ctx.emitter.instruction(&format!("jbe {}", next_label));                    // accept uppercase ASCII letters
}

/// Emits an x86_64 lowercase ASCII range probe that branches on success.
fn emit_x86_64_lowercase_accept(ctx: &mut FunctionContext<'_>, next_label: &str) {
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before normalizing against lowercase ASCII
    ctx.emitter.instruction("sub r9d, 97");                                     // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp r9d, 25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("jbe {}", next_label));                    // accept lowercase ASCII letters
}

/// Emits an x86_64 lowercase ASCII range probe that branches on failure.
fn emit_x86_64_lowercase_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
    ctx.emitter.instruction("mov r9d, r8d");                                    // copy the byte before normalizing against lowercase ASCII
    ctx.emitter.instruction("sub r9d, 97");                                     // normalize the byte against lowercase ASCII 'a'
    ctx.emitter.instruction("cmp r9d, 25");                                     // check whether the byte is inside a-z
    ctx.emitter.instruction(&format!("ja {}", fail_label));                     // reject bytes outside lowercase ASCII after uppercase failed
}

/// Emits an x86_64 digit ASCII range probe that branches on failure.
fn emit_x86_64_digit_reject(ctx: &mut FunctionContext<'_>, fail_label: &str) {
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
