//! Purpose:
//! Emits PHP `stream_socket_accept` calls.
//! Accepts a pending connection on a listening socket, optionally with a
//! timeout, and captures the peer address for the by-reference
//! `$peer_name` out-parameter.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Marshals (fd, timeout_us) into the helper. A missing/null timeout
//!   becomes `-1`, signalling an infinite wait. A numeric timeout is
//!   multiplied by `1_000_000` so the helper can call `select()` /
//!   `pselect6()` with a single integer microsecond argument.
//! - The accepted descriptor (or `-1`) is boxed by the shared
//!   `box_socket_result` helper into a Mixed cell. When the caller passed
//!   a `&$peer_name` variable the address stashed in
//!   `_accept_peer_ptr` / `_accept_peer_len` is copied into its slot.

use crate::codegen_support::builtins::io::stream_arg::emit_stream_fd_arg;
use crate::codegen_support::builtins::io::stream_socket_server::box_socket_result;
use crate::codegen_support::context::{Context, HeapOwnership};
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits codegen for PHP `stream_socket_accept()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_socket_accept()");
    emit_stream_fd_arg("stream_socket_accept", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the descriptor
    emit_timeout_us(args.get(1), emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // timeout_us into argument 1
            abi::emit_pop_reg(emitter, "x0"); // descriptor into argument 0
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // timeout_us into argument 1
            abi::emit_pop_reg(emitter, "rdi"); // descriptor into argument 0
        }
    }
    abi::emit_call_label(emitter, "__rt_stream_socket_accept");
    box_socket_result(emitter, ctx);
    if let Some(peer_arg) = args.get(2) {
        emit_store_peer_name(peer_arg, emitter, ctx);
    }
    Some(PhpType::Mixed)
}

/// Evaluates the optional timeout argument and leaves an i64 microsecond
/// count in the int result register. A missing or `null` timeout lowers to
/// `-1` (the helper's "infinite wait" sentinel). A numeric timeout is taken
/// as a count of seconds and converted to microseconds with a 1_000_000
/// multiplier so PHP code can pass either an int or a float-shaped int.
fn emit_timeout_us(
    timeout: Option<&Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let infinite_sentinel = |emitter: &mut Emitter| match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("mov x0, #-1"),                    // sentinel: infinite wait
        Arch::X86_64 => emitter.instruction("mov rax, -1"),                     // sentinel: infinite wait
    };
    let Some(expr) = timeout else {
        infinite_sentinel(emitter);
        return;
    };
    if matches!(&expr.kind, ExprKind::Null) {
        infinite_sentinel(emitter);
        return;
    }
    emit_expr(expr, emitter, ctx, data);
    // PHP exposes the timeout as a (possibly fractional) seconds value; elephc
    // ints round toward zero, which matches PHP integer behaviour when the
    // caller passes (int)$timeout. Scale to microseconds for the helper.
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, #0x4240");                             // low 16 bits of 1_000_000 (0xF4240)
            emitter.instruction("movk x9, #0xF, lsl #16");                      // upper bits make x9 = 1_000_000 (one second in us)
            emitter.instruction("mul x0, x0, x9");                              // timeout_us = timeout_sec * 1_000_000
        }
        Arch::X86_64 => {
            emitter.instruction("imul rax, rax, 1000000");                      // timeout_us = timeout_sec * 1_000_000
        }
    }
}

/// Copies the peer address (stashed by `__rt_stream_socket_accept` in the
/// `_accept_peer_*` globals) into the by-reference `$peer_name` variable.
/// Modelled on `stream_socket_recvfrom`'s storage-class dispatch.
fn emit_store_peer_name(arg: &Expr, emitter: &mut Emitter, ctx: &mut Context) {
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(emitter, "x0"); // preserve the boxed accept result
            abi::emit_symbol_address(emitter, "x9", "_accept_peer_ptr");
            emitter.instruction("ldr x10, [x9]");                               // load the stashed peer address pointer
            abi::emit_symbol_address(emitter, "x9", "_accept_peer_len");
            emitter.instruction("ldr x11, [x9]");                               // load the stashed peer address length
            emit_store_peer_slot(name, emitter, ctx);
            abi::emit_pop_reg(emitter, "x0"); // restore the boxed accept result
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax"); // preserve the boxed accept result
            abi::emit_symbol_address(emitter, "r9", "_accept_peer_ptr");        // address of the stashed-pointer global
            emitter.instruction("mov r10, QWORD PTR [r9]");                     // load the stashed peer address pointer
            abi::emit_symbol_address(emitter, "r9", "_accept_peer_len");        // address of the stashed-length global
            emitter.instruction("mov r11, QWORD PTR [r9]");                     // load the stashed peer address length
            emit_store_peer_slot(name, emitter, ctx);
            abi::emit_pop_reg(emitter, "rax"); // restore the boxed accept result
        }
    }
    ctx.update_var_type_and_ownership(name, PhpType::Str, HeapOwnership::Owned);
}

/// Stores the peer address (pointer in x10/r10, length in x11/r11) into the
/// `$peer_name` variable's 16-byte string slot, dispatching on storage class.
fn emit_store_peer_slot(name: &str, emitter: &mut Emitter, ctx: &Context) {
    let is_global =
        ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name));
    if is_global {
        let label = format!("_gvar_{}", name);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x9", &label);                // load page of the global address variable
                emitter.instruction("str x10, [x9]");                           // store the address string pointer
                emitter.instruction("str x11, [x9, #8]");                       // store the address string length
            }
            Arch::X86_64 => {
                abi::emit_store_reg_to_symbol(emitter, "r10", &label, 0);        // store the address string pointer
                abi::emit_store_reg_to_symbol(emitter, "r11", &label, 8);        // store the address string length
            }
        }
        return;
    }
    if ctx.ref_params.contains(name) {
        let offset = ctx
            .variables
            .get(name)
            .expect("codegen bug: missing ref-param slot for accept $peer_name")
            .stack_offset;
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::load_at_offset(emitter, "x9", offset);                     // load the referenced address storage pointer
                emitter.instruction("str x10, [x9]");                           // store the address string pointer
                emitter.instruction("str x11, [x9, #8]");                       // store the address string length
            }
            Arch::X86_64 => {
                abi::load_at_offset(emitter, "r9", offset);                     // load the referenced address storage pointer
                abi::emit_store_to_address(emitter, "r10", "r9", 0);            // store the address string pointer
                abi::emit_store_to_address(emitter, "r11", "r9", 8);            // store the address string length
            }
        }
        return;
    }
    if let Some(offset) = ctx.variables.get(name).map(|var| var.stack_offset) {
        // A local string slot keeps the pointer at `offset` and the length at
        // `offset - 8`, matching `abi::emit_store`/`emit_load` for `PhpType::Str`.
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::store_at_offset(emitter, "x10", offset);                   // store the address string pointer
                abi::store_at_offset(emitter, "x11", offset - 8);               // store the address string length
            }
            Arch::X86_64 => {
                abi::store_at_offset(emitter, "r10", offset);                   // store the address string pointer
                abi::store_at_offset(emitter, "r11", offset - 8);               // store the address string length
            }
        }
    }
}
