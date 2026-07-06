//! Purpose:
//! Emits PHP `stream_filter_register` calls.
//! Records a `(filter_name, class_name)` pair in the runtime user-filter
//! registry that `stream_filter_append`/`prepend` consult on attachment
//! and `__rt_apply_stream_filter` dispatches into on read/write.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The registry stores up to 128 registrations.
//!   On success the runtime helper returns `true`; on a full table it
//!   returns `false`. The wrapper class is invoked through the per-class
//!   `_user_filter_vtable_<class_id>` (slot 0 = filter, 1 = onCreate,
//!   2 = onClose) — the elephc v1 contract is `filter(string): string`,
//!   not the PHP bucket-brigade signature.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for PHP `stream_filter_register()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_filter_register()");
    // PHP evaluates the filter name first, then the class name. The two
    // strings are handed to the runtime helper.
    emit_expr(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2");                       // preserve the filter-name string ptr/len
            emit_expr(&args[1], emitter, ctx, data);
            // After emit_expr, x1/x2 hold the class-name string. The helper
            // expects x0=name_ptr x1=name_len x2=class_ptr x3=class_len.
            // Move class_len into x3 first so the class_ptr → x2 mov does
            // not clobber the original x2.
            emitter.instruction("mov x3, x2");                                  // class-name length into x3
            emitter.instruction("mov x2, x1");                                  // class-name pointer into x2
            abi::emit_pop_reg_pair(emitter, "x0", "x1");                        // restore filter-name ptr/len
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the filter-name string ptr/len
            emit_expr(&args[1], emitter, ctx, data);
            // x86_64 helper expects rdi=name_ptr rsi=name_len rdx=class_ptr
            // rcx=class_len. The class string is in rax/rdx after emit_expr.
            emitter.instruction("mov rcx, rdx");                                // class-name length into rcx
            emitter.instruction("mov rdx, rax");                                // class-name pointer into rdx
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore filter-name ptr/len
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_filter_register");
    Some(PhpType::Bool)
}
