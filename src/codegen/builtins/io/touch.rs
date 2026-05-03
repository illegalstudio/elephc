use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::functions::infer_contextual_type;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

const TOUCH_ATIME_NOW: u8 = 1;
const TOUCH_MTIME_NOW: u8 = 2;
const TOUCH_BOTH_NOW: u8 = TOUCH_ATIME_NOW | TOUCH_MTIME_NOW;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("touch()");
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => emit_touch_args_aarch64(args, emitter, ctx, data),
        Arch::X86_64 => emit_touch_args_x86_64(args, emitter, ctx, data),
    }
    abi::emit_call_label(emitter, "__rt_touch");                                // call the target-aware runtime helper
    Some(PhpType::Bool)
}

fn emit_touch_args_aarch64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match touch_time_shape(args, ctx) {
        TouchTimeShape::BothNow => {
            emitter.instruction("mov x3, #0");                                  // ignored mtime seconds when runtime uses current time
            emitter.instruction("mov x4, #0");                                  // ignored atime seconds when runtime uses current time
            emitter.instruction(&format!("mov x5, #{}", TOUCH_BOTH_NOW));       // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path while mtime is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0");                                  // mtime seconds
            emitter.instruction("mov x4, x0");                                  // atime defaults to mtime seconds
            emitter.instruction("mov x5, #0");                                  // both timestamp fields are explicit
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore path ptr/len
        }
        TouchTimeShape::ExplicitBoth => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve path while timestamps are evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // save mtime seconds
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov x4, x0");                                  // atime seconds
            emitter.instruction("ldr x3, [sp], #16");                           // restore mtime seconds
            emitter.instruction("mov x5, #0");                                  // both timestamp fields are explicit
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore path ptr/len
        }
    }
}

fn emit_touch_args_x86_64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match touch_time_shape(args, ctx) {
        TouchTimeShape::BothNow => {
            emitter.instruction("mov rdi, 0");                                  // ignored mtime seconds when runtime uses current time
            emitter.instruction("mov rsi, 0");                                  // ignored atime seconds when runtime uses current time
            emitter.instruction(&format!("mov rcx, {}", TOUCH_BOTH_NOW));       // mark mtime and atime as current-time fields
        }
        TouchTimeShape::MtimeAlsoAtime => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path while mtime is evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov rdi, rax");                                // mtime seconds
            emitter.instruction("mov rsi, rax");                                // atime defaults to mtime seconds
            emitter.instruction("mov rcx, 0");                                  // both timestamp fields are explicit
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore path ptr/len
        }
        TouchTimeShape::ExplicitBoth => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve path while timestamps are evaluated
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("sub rsp, 16");                                 // reserve aligned temporary storage for mtime
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // save mtime seconds
            emit_expr(&args[2], emitter, ctx, data);
            emitter.instruction("mov rsi, rax");                                // atime seconds
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // restore mtime seconds
            emitter.instruction("add rsp, 16");                                 // release mtime temporary storage
            emitter.instruction("mov rcx, 0");                                  // both timestamp fields are explicit
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore path ptr/len
        }
    }
}

enum TouchTimeShape {
    BothNow,
    MtimeAlsoAtime,
    ExplicitBoth,
}

fn touch_time_shape(args: &[Expr], ctx: &Context) -> TouchTimeShape {
    match args.len() {
        1 => TouchTimeShape::BothNow,
        2 if is_static_null(&args[1], ctx) => TouchTimeShape::BothNow,
        2 => TouchTimeShape::MtimeAlsoAtime,
        _ if is_static_null(&args[1], ctx) && is_static_null(&args[2], ctx) => {
            TouchTimeShape::BothNow
        }
        _ if is_static_null(&args[2], ctx) => TouchTimeShape::MtimeAlsoAtime,
        _ => TouchTimeShape::ExplicitBoth,
    }
}

fn is_static_null(expr: &Expr, ctx: &Context) -> bool {
    matches!(expr.kind, ExprKind::Null) || infer_contextual_type(expr, ctx) == PhpType::Void
}
