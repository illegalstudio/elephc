//! Purpose:
//! Rejects unsupported Fiber suspension at the shared Parallel descriptor boundary.
//!
//! Called from:
//! - The descriptor invoker before argument marshalling, after installing its cleanup boundary.
//!
//! Key details:
//! - Native higher-order callbacks bypass ordinary EIR call-site guards.
//! - Scheduler-owned direct Fiber yields are not descriptor invocations.

use super::*;

/// Checks descriptor kind/name and applies the worker-local suspension backstop.
pub(super) fn emit_parallel_fiber_suspend_descriptor_guard(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut InvokerEmitContext,
    task_group_id: u64,
    assert_symbol: &str,
) {
    let descriptor_reg = match emitter.target.arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    };
    let name_ptr = match emitter.target.arch {
        Arch::AArch64 => "x20",
        Arch::X86_64 => "r13",
    };
    let name_len = match emitter.target.arch {
        Arch::AArch64 => "x21",
        Arch::X86_64 => "r15",
    };
    abi::load_at_offset(emitter, descriptor_reg, INVOKER_DESCRIPTOR_OFFSET);
    let done = ctx.next_label("parallel_fiber_suspend_guard_done");
    let compare_name = ctx.next_label("parallel_fiber_suspend_compare_name");
    let kind_reg = match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r9",
    };
    abi::emit_load_from_address(emitter, kind_reg, descriptor_reg, 0);
    for kind in [
        callable_descriptor::CALLABLE_DESC_KIND_CLOSURE,
        callable_descriptor::CALLABLE_DESC_KIND_FIRST_CLASS,
        callable_descriptor::CALLABLE_DESC_KIND_CALLBACK_ADAPTER,
        callable_descriptor::CALLABLE_DESC_KIND_STATIC_METHOD,
        callable_descriptor::CALLABLE_DESC_KIND_INSTANCE_METHOD,
    ] {
        abi::emit_load_int_immediate(
            emitter,
            abi::int_arg_reg_name(emitter.target, 0),
            kind as i64,
        );
        emitter.instruction(&format!("cmp {kind_reg}, {}", abi::int_arg_reg_name(emitter.target, 0))); // compare the descriptor kind with a PHP-callable shape
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("b.eq {compare_name}")), // inspect PHP names only for supported descriptor kinds
            Arch::X86_64 => emitter.instruction(&format!("je {compare_name}")), // inspect PHP names only for supported descriptor kinds
        }
    }
    abi::emit_jump(emitter, &done);
    emitter.label(&compare_name);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "ldr {name_ptr}, [{descriptor_reg}, #{}]",
                callable_descriptor::CALLABLE_DESC_PHP_NAME_OFFSET
            ));                                                                 // load the PHP-visible descriptor name pointer
            emitter.instruction(&format!(
                "ldr {name_len}, [{descriptor_reg}, #{}]",
                callable_descriptor::CALLABLE_DESC_PHP_NAME_LEN_OFFSET
            ));                                                                 // load the PHP-visible descriptor name byte length
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov {name_ptr}, QWORD PTR [{descriptor_reg} + {}]",
                callable_descriptor::CALLABLE_DESC_PHP_NAME_OFFSET
            ));                                                                 // load the PHP-visible descriptor name pointer
            emitter.instruction(&format!(
                "mov {name_len}, QWORD PTR [{descriptor_reg} + {}]",
                callable_descriptor::CALLABLE_DESC_PHP_NAME_LEN_OFFSET
            ));                                                                 // load the PHP-visible descriptor name byte length
        }
    }

    for candidate in [b"Fiber::suspend".as_slice(), b"\\Fiber::suspend".as_slice()] {
        let next = ctx.next_label("parallel_fiber_suspend_guard_next");
        let (candidate_label, candidate_len) = data.add_string(candidate);
        let scratch = match emitter.target.arch {
            Arch::AArch64 => "x9",
            Arch::X86_64 => "r9",
        };
        abi::emit_load_int_immediate(emitter, scratch, candidate_len as i64);
        emitter.instruction(&format!("cmp {name_len}, {scratch}"));             // reject spellings with a different byte length
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("b.ne {next}")),      // try the next spelling when its length differs
            Arch::X86_64 => emitter.instruction(&format!("jne {next}")),        // try the next spelling when its length differs
        }

        abi::emit_push_reg_pair(emitter, name_ptr, name_len);
        match emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x3", &candidate_label);
                abi::emit_load_int_immediate(emitter, "x4", candidate_len as i64);
                abi::emit_load_temporary_stack_slot(emitter, "x1", 0);
                abi::emit_load_temporary_stack_slot(emitter, "x2", 8);
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(emitter, "rdx", &candidate_label);
                abi::emit_load_int_immediate(emitter, "rcx", candidate_len as i64);
                abi::emit_load_temporary_stack_slot(emitter, "rdi", 0);
                abi::emit_load_temporary_stack_slot(emitter, "rsi", 8);
            }
        }
        abi::emit_call_label(emitter, "__rt_strcasecmp");
        let mismatch = ctx.next_label("parallel_fiber_suspend_guard_mismatch");
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // test the case-insensitive PHP-name comparison result
                emitter.instruction(&format!("b.ne {mismatch}"));               // restore scratch and continue after a name mismatch
            }
            Arch::X86_64 => {
                emitter.instruction("test rax, rax");                           // test the case-insensitive PHP-name comparison result
                emitter.instruction(&format!("jne {mismatch}"));                // restore scratch and continue after a name mismatch
            }
        }
        abi::emit_release_temporary_stack(emitter, 16);
        abi::emit_load_int_immediate(
            emitter,
            abi::int_arg_reg_name(emitter.target, 0),
            task_group_id as i64,
        );
        abi::emit_call_label(emitter, assert_symbol);
        abi::emit_jump(emitter, &done);

        // The mismatch branch restores the temporary name pair before trying the next spelling.
        emitter.label(&mismatch);
        abi::emit_release_temporary_stack(emitter, 16);
        abi::emit_jump(emitter, &next);
        emitter.label(&next);
    }
    emitter.label(&done);
}
