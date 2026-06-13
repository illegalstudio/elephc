//! Purpose:
//! Lowers typed buffer element reads using pointer arithmetic and element sizes.
//! Produces expression results while preserving container ownership and bounds/null behavior.
//!
//! Called from:
//! - `crate::codegen::expr::arrays::access`
//!
//! Key details:
//! - Element layout and boxed Mixed handling must stay aligned with array runtime helpers.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, TypeExpr};
use crate::types::{packed_type_size, PhpType};

/// Allocates a new buffer with `element_type` elements of the given length expression.
///
/// The length is evaluated first, then the element stride is loaded into the appropriate
/// integer argument register for the target architecture before calling `__rt_buffer_new`.
/// Returns `PhpType::Buffer` wrapping the resolved element type.
pub(crate) fn emit_buffer_new(
    element_type: &TypeExpr,
    len: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let len_ty = emit_expr(len, emitter, ctx, data);
    let elem_ty = resolve_buffer_element_type(element_type, ctx);
    let stride = packed_type_size(&elem_ty, &ctx.packed_classes).unwrap_or(8);
    if len_ty != PhpType::Int {
        emitter.comment("WARNING: buffer_new length was not statically typed as int");
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x1, #{}", stride));               // pass the element stride to the ARM buffer allocation helper in the second integer argument register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", stride));               // pass the element stride to the x86_64 buffer allocation helper without clobbering the computed length in rax
        }
    }
    abi::emit_call_label(emitter, "__rt_buffer_new");                           // allocate the buffer header plus contiguous payload through the target-aware runtime helper
    PhpType::Buffer(Box::new(elem_ty))
}

/// Resolves a `TypeExpr` to a `PhpType` for buffer element type checking.
///
/// Unpacked named types are resolved against the packed class table; unknown named types
/// fall back to `PhpType::Int`. Nullable, union, and iterable types are not valid buffer
/// elements and resolve to `PhpType::Int` as a safe approximation.
fn resolve_buffer_element_type(type_expr: &TypeExpr, ctx: &Context) -> PhpType {
    match type_expr {
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Ptr(target) => {
            PhpType::Pointer(target.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Named(name) => {
            if ctx.packed_classes.contains_key(name.as_str()) {
                PhpType::Packed(name.as_str().to_string())
            } else {
                PhpType::Int
            }
        }
        TypeExpr::Buffer(inner) => {
            PhpType::Buffer(Box::new(resolve_buffer_element_type(inner, ctx)))
        }
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Array(_)
        | TypeExpr::Nullable(_)
        | TypeExpr::Union(_)
        | TypeExpr::Intersection(_)
        | TypeExpr::Iterable => PhpType::Int,
    }
}
