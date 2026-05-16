//! Purpose:
//! Emits codegen for `is_callable()`.
//! Handles compile-time callable shapes and delegates dynamic PHP callable forms to runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()` when lowering type/introspection builtins.
//!
//! Key details:
//! - Runtime fallback covers non-literal strings, callable arrays, invokable objects, Mixed, and erased iterables.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::checker::builtins::is_supported_builtin_function;
use crate::types::PhpType;

/// is_callable(value): bool
///
/// Static evaluation when the argument's compile-time type is Callable
/// (closures, first-class callables) or a string literal that resolves
/// to a known builtin / user function. Dynamic strings, callable arrays,
/// objects, and type-erased payloads route to runtime metadata lookup.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_callable()");

    // Compile-time string literal: defer to the same lookup as
    // function_exists() — known catalog builtin or user-declared
    // function ⇒ true, else false. Evaluating the literal expression
    // has no side effects, so we skip emit_expr.
    if let ExprKind::StringLiteral(name) = &args[0].kind {
        if !name.contains("::") {
            let known = ctx.functions.contains_key(name)
                || is_supported_builtin_function(name)
                || ctx.function_variant_groups.contains(name);
            let val: i64 = if known { 1 } else { 0 };
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), val);
            return Some(PhpType::Bool);
        }
    }

    let ty = emit_expr(&args[0], emitter, ctx, data);
    match ty.codegen_repr() {
        PhpType::Callable => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
        }
        PhpType::Str => emit_dynamic_string_lookup(emitter),
        PhpType::Array(_) => {
            emit_pointer_lookup(emitter, "__rt_is_callable_array");             // inspect indexed array shape for callable arrays
        }
        PhpType::AssocArray { .. } => {
            emit_pointer_lookup(emitter, "__rt_is_callable_assoc");             // inspect hash shape for numeric 0/1 callable-array entries
        }
        PhpType::Object(_) => {
            emit_pointer_lookup(emitter, "__rt_is_callable_object");            // check whether the object's runtime class exposes public __invoke
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_pointer_lookup(emitter, "__rt_is_callable_mixed");             // unwrap Mixed and dispatch to the dynamic callable checks
        }
        PhpType::Iterable => {
            emit_pointer_lookup(emitter, "__rt_is_callable_heap");              // inspect erased iterable heap kind before choosing array/object fallback
        }
        _ => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
    }
    Some(PhpType::Bool)
}

fn emit_pointer_lookup(emitter: &mut Emitter, label: &str) {
    if emitter.target.arch == crate::codegen::platform::Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move pointer-shaped result into SysV helper argument 0
    }
    abi::emit_call_label(emitter, label);                                       // call the selected pointer-shaped runtime callable fallback
}

fn emit_dynamic_string_lookup(emitter: &mut Emitter) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // move dynamic string pointer into runtime helper argument 0
            emitter.instruction("mov x1, x2");                                  // move dynamic string length into runtime helper argument 1
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move dynamic string pointer into SysV helper argument 0
            emitter.instruction("mov rsi, rdx");                                // move dynamic string length into SysV helper argument 1
        }
    }
    abi::emit_call_label(emitter, "__rt_is_callable_string");                   // resolve dynamic function-name string against builtin and user metadata
}
