//! Purpose:
//! Lowers PHP `ctype_*` character-class builtins for the EIR backend.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_09::lower()`.
//!
//! Key details:
//! - PHP 8.1+ interprets a non-string argument as a character code and emits an
//!   `E_DEPRECATED` naming the argument's runtime type; this lowering preserves that exact
//!   split instead of coercing every operand to `string`.
//! - Strings scan every byte, return false for the empty string, and never warn.
//! - `Mixed`/nullable-int operands dispatch at run time through `__rt_ctype_check`, which
//!   owns the tag-based type-name selection and the caller-location diagnostic.
//! - The shared runtime helpers own the byte predicates and the deprecation text so the
//!   string, integer, and boxed paths cannot drift apart.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::Instruction;
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{ensure_arg_count, expect_operand, store_if_result};

/// Lowers `ctype_alnum(mixed $text)`.
pub(crate) fn lower_ctype_alnum(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Alnum)
}

/// Lowers `ctype_alpha(mixed $text)`.
pub(crate) fn lower_ctype_alpha(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Alpha)
}

/// Lowers `ctype_cntrl(mixed $text)`.
pub(crate) fn lower_ctype_cntrl(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Cntrl)
}

/// Lowers `ctype_digit(mixed $text)`.
pub(crate) fn lower_ctype_digit(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Digit)
}

/// Lowers `ctype_graph(mixed $text)`.
pub(crate) fn lower_ctype_graph(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Graph)
}

/// Lowers `ctype_lower(mixed $text)`.
pub(crate) fn lower_ctype_lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Lower)
}

/// Lowers `ctype_print(mixed $text)`.
pub(crate) fn lower_ctype_print(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Print)
}

/// Lowers `ctype_punct(mixed $text)`.
pub(crate) fn lower_ctype_punct(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Punct)
}

/// Lowers `ctype_space(mixed $text)`.
pub(crate) fn lower_ctype_space(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Space)
}

/// Lowers `ctype_upper(mixed $text)`.
pub(crate) fn lower_ctype_upper(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Upper)
}

/// Lowers `ctype_xdigit(mixed $text)`.
pub(crate) fn lower_ctype_xdigit(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_ctype(ctx, inst, CtypeKind::Xdigit)
}

/// Identifies the byte predicate and scalar fallback flags implemented by one ctype builtin.
#[derive(Clone, Copy)]
enum CtypeKind {
    Alnum,
    Alpha,
    Cntrl,
    Digit,
    Graph,
    Lower,
    Print,
    Punct,
    Space,
    Upper,
    Xdigit,
}

impl CtypeKind {
    /// Returns the kind index shared with the runtime predicate/dispatch helpers.
    fn index(self) -> i64 {
        match self {
            CtypeKind::Alnum => 0,
            CtypeKind::Alpha => 1,
            CtypeKind::Cntrl => 2,
            CtypeKind::Digit => 3,
            CtypeKind::Graph => 4,
            CtypeKind::Lower => 5,
            CtypeKind::Print => 6,
            CtypeKind::Punct => 7,
            CtypeKind::Space => 8,
            CtypeKind::Upper => 9,
            CtypeKind::Xdigit => 10,
        }
    }

    /// Returns the PHP-visible builtin name for arity diagnostics.
    fn name(self) -> &'static str {
        match self {
            CtypeKind::Alnum => "ctype_alnum",
            CtypeKind::Alpha => "ctype_alpha",
            CtypeKind::Cntrl => "ctype_cntrl",
            CtypeKind::Digit => "ctype_digit",
            CtypeKind::Graph => "ctype_graph",
            CtypeKind::Lower => "ctype_lower",
            CtypeKind::Print => "ctype_print",
            CtypeKind::Punct => "ctype_punct",
            CtypeKind::Space => "ctype_space",
            CtypeKind::Upper => "ctype_upper",
            CtypeKind::Xdigit => "ctype_xdigit",
        }
    }
}

/// Dispatches one ctype call on the operand's PHP representation.
fn lower_ctype(ctx: &mut FunctionContext<'_>, inst: &Instruction, kind: CtypeKind) -> Result<()> {
    ensure_arg_count(inst, kind.name(), 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    // `Resource` codegen-reduces to an integer word, but PHP must still name it a
    // resource and answer false, so the declared type is checked before the
    // representation dispatch below.
    if matches!(ty, PhpType::Resource(_)) {
        emit_static_deprecation(ctx, kind, "resource");
        emit_false(ctx);
        return store_if_result(ctx, inst);
    }
    // A Closure is an object for PHP's type name but its runtime payload is a callable
    // descriptor, not a class-id-bearing object header, so it keeps the static name.
    if matches!(&ty, PhpType::Object(name) if name.eq_ignore_ascii_case("Closure")) {
        emit_static_deprecation(ctx, kind, "Closure");
        emit_false(ctx);
        return store_if_result(ctx, inst);
    }
    // Objects are named by their CONCRETE class at runtime; a `Base`-typed operand may
    // hold a subclass, and php-src's `zend_zval_type_name()` reports the subclass.
    if matches!(ty, PhpType::Object(_)) {
        lower_object(ctx, kind, value)?;
        return store_if_result(ctx, inst);
    }
    match ty.codegen_repr() {
        PhpType::Str => lower_string(ctx, kind, value)?,
        PhpType::Int => lower_int(ctx, kind, value)?,
        PhpType::TaggedScalar => lower_tagged_scalar(ctx, kind, value)?,
        PhpType::Mixed | PhpType::Union(_) => lower_mixed(ctx, kind, value)?,
        repr => {
            emit_static_deprecation(ctx, kind, &static_type_name(&ty, &repr));
            emit_false(ctx);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a concrete string operand through the shared byte scanner.
fn lower_string(ctx: &mut FunctionContext<'_>, kind: CtypeKind, value: crate::ir::ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // String results arrive in x1 (pointer) and x2 (length); the kind belongs in x0.
            abi::emit_load_int_immediate(ctx.emitter, "x0", kind.index());
        }
        Arch::X86_64 => {
            // String results arrive in rax (pointer) and rdx (length). The runtime helper
            // takes its own uniform integer ABI: kind, pointer, length.
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string pointer as the helper's second argument
            abi::emit_load_int_immediate(ctx.emitter, "rdi", kind.index());
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ctype_string");
    Ok(())
}

/// Lowers a concrete integer operand, preserving PHP's deprecated fallback rules.
fn lower_int(ctx: &mut FunctionContext<'_>, kind: CtypeKind, value: crate::ir::ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    spill_int_result(ctx);
    emit_static_deprecation(ctx, kind, "int");
    restore_int_result_to_second_arg(ctx, kind);
    abi::emit_call_label(ctx.emitter, "__rt_ctype_int");
    Ok(())
}

/// Lowers a nullable-int inline tagged scalar by selecting null versus integer at run time.
fn lower_tagged_scalar(
    ctx: &mut FunctionContext<'_>,
    kind: CtypeKind,
    value: crate::ir::ValueId,
) -> Result<()> {
    let null_label = ctx.next_label("ctype_tagged_null");
    let done_label = ctx.next_label("ctype_tagged_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_label);
    spill_int_result(ctx);
    emit_static_deprecation(ctx, kind, "int");
    restore_int_result_to_second_arg(ctx, kind);
    abi::emit_call_label(ctx.emitter, "__rt_ctype_int");
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&null_label);
    emit_static_deprecation(ctx, kind, "null");
    emit_false(ctx);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers a concrete object operand through the runtime class-name deprecation.
fn lower_object(ctx: &mut FunctionContext<'_>, kind: CtypeKind, value: crate::ir::ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the borrowed object pointer as the helper's second argument
            abi::emit_load_int_immediate(ctx.emitter, "x0", kind.index());
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the borrowed object pointer as the helper's second argument
            abi::emit_load_int_immediate(ctx.emitter, "rdi", kind.index());
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ctype_object");
    Ok(())
}

/// Lowers a boxed Mixed operand through the tag-aware runtime dispatcher.
fn lower_mixed(ctx: &mut FunctionContext<'_>, kind: CtypeKind, value: crate::ir::ValueId) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the borrowed Mixed cell as the checker's second argument
            abi::emit_load_int_immediate(ctx.emitter, "x0", kind.index());
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the borrowed Mixed cell as the checker's second argument
            abi::emit_load_int_immediate(ctx.emitter, "rdi", kind.index());
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ctype_check");
    Ok(())
}

/// Preserves the evaluated integer/scalar result across the deprecation callback.
fn spill_int_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("sub sp, sp, #16");                         // keep the temporary word 16-byte aligned across the diagnostic
            ctx.emitter.instruction("str x0, [sp]");                            // preserve the evaluated argument before the deprecation can run a handler
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("sub rsp, 16");                             // keep the temporary word aligned across the diagnostic
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // preserve the evaluated argument before the deprecation can run a handler
        }
    }
}

/// Restores the preserved result into the integer helper's second argument register.
fn restore_int_result_to_second_arg(ctx: &mut FunctionContext<'_>, kind: CtypeKind) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x1, [sp]");                            // restore the evaluated integer as the helper's value argument
            ctx.emitter.instruction("add sp, sp, #16");                         // release the temporary word after restoring the value
            abi::emit_load_int_immediate(ctx.emitter, "x0", kind.index());
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                // restore the evaluated integer as the helper's value argument
            ctx.emitter.instruction("add rsp, 16");                             // release the temporary word after restoring the value
            abi::emit_load_int_immediate(ctx.emitter, "rdi", kind.index());
        }
    }
}

/// Emits the php-src deprecation for an argument whose type is known statically.
fn emit_static_deprecation(ctx: &mut FunctionContext<'_>, kind: CtypeKind, type_name: &str) {
    let (label, len) = ctx.data.add_string(type_name.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", kind.index());
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", kind.index());
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_ctype_deprecated");
}

/// Materializes PHP `false` in the integer result register.
fn emit_false(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
}

/// Returns the `zend_zval_type_name()` spelling for a statically known argument shape.
fn static_type_name(declared: &PhpType, repr: &PhpType) -> String {
    match declared {
        PhpType::Object(name) => name.clone(),
        PhpType::Callable => "Closure".to_string(),
        PhpType::Resource(_) => "resource".to_string(),
        PhpType::Iterable => "array".to_string(),
        PhpType::Packed(_) => "object".to_string(),
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Narrow(_) => "int".to_string(),
        _ => match repr {
            PhpType::Float => "float".to_string(),
            PhpType::Bool | PhpType::False => "bool".to_string(),
            PhpType::Void | PhpType::Never => "null".to_string(),
            PhpType::Array(_) | PhpType::AssocArray { .. } => "array".to_string(),
            PhpType::Object(name) => name.clone(),
            PhpType::Callable => "Closure".to_string(),
            PhpType::Str => "string".to_string(),
            _ => "unknown".to_string(),
        },
    }
}
