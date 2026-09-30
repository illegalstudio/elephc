//! Purpose:
//! Lowers typed EIR runtime operations after target selection and value placement.
//! Owns concrete helper symbols and physical calling-convention materialization.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_runtime_call()` for typed `RuntimeCall` immediates.
//!
//! Key details:
//! - PHP builtin names never participate in dispatch.
//! - Every typed call validates its EIR signature before emitting a helper call.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, RuntimeCallTarget, UnaryStringRuntime};
use crate::types::PhpType;

use super::receiver_place::ReceiverPlace;
use super::{expect_operand, store_if_result};

/// Tells the monitor that this call is a STREAM operation, naming it.
///
/// One place rather than one per builtin: every typed runtime call arrives here, so the
/// counter cannot fall behind a builtin somebody adds later — the only thing that decides
/// is `RuntimeFnId::is_stream_operation`, which is the list.
///
/// Emitted ONLY under `--with-monitoring`. The slot it would read is zero in every other
/// binary, so the guarded call would be inert — but "inert" still costs a load, a branch and
/// the instructions around them at every read in every loop, and a build nobody asked to
/// profile should not carry that. This is the same pay-for-use rule the profiler's own
/// enter/exit hooks follow.
///
/// The name is passed rather than an id because the reader wants it: a function that did
/// 1,200 stream operations is a different finding depending on whether that is one `fopen`
/// and 1,199 `fgets` — a read loop — or 1,200 `fopen` calls.
///
/// Nothing of the call's own is live yet. This runs BEFORE the operands are materialized into
/// argument registers, which is what makes clobbering them safe.
fn emit_stream_operation_note(ctx: &mut FunctionContext<'_>, target: RuntimeCallTarget) {
    if !ctx.shared.instrument.is_on() {
        return;
    }
    let (RuntimeCallTarget::Function(id) | RuntimeCallTarget::ProfiledFunction { target: id, .. }) =
        target
    else {
        return;
    };
    if !id.is_stream_operation() {
        return;
    }
    let name = id.as_eir();
    let (label, len) = ctx.data.add_string(name.as_bytes());
    let arch_target = ctx.emitter.target;
    let symbol = arch_target.extern_symbol("elephc_instr_stream_fn");
    let skip = ctx.next_label("instr_stream_skip");
    // The symbol scratch register: the load below borrows it for the address and
    // then holds the slot's value, and neither argument register materialized after
    // it touches it, so it still holds the callee at the indirect call.
    let slot = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, slot, &symbol, 0);
    match arch_target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", slot, skip));    // dormant: the stream slot is zero, so skip the note
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", slot, slot));   // is the stream slot filled?
            ctx.emitter.instruction(&format!("jz {}", skip));               // dormant: the stream slot is zero, so skip the note
        }
    }
    let name_arg = abi::int_arg_reg_name(arch_target, 0);
    let len_arg = abi::int_arg_reg_name(arch_target, 1);
    abi::emit_symbol_address(ctx.emitter, name_arg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_arg, len as i64);
    abi::emit_call_reg(ctx.emitter, slot); // elephc_instr_stream(name, len)
    ctx.emitter.label(&skip);
}

/// Lowers one typed runtime operation through its target-specific helper ABI.
pub(super) fn lower(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    target: RuntimeCallTarget,
) -> Result<()> {
    emit_stream_operation_note(ctx, target);
    match target {
        RuntimeCallTarget::ThrowableInitialize => lower_throwable_initialize(ctx, inst),
        RuntimeCallTarget::ArrayFetchForWrite => {
            super::lower_array_fetch_for_write_runtime_call(ctx, inst)
        }
        RuntimeCallTarget::MixedCellPromoteToHash(sort) => {
            lower_mixed_cell_promote_to_hash(ctx, inst, sort, false)
        }
        RuntimeCallTarget::MixedCellPromoteAttachedToHash(sort) => {
            lower_mixed_cell_promote_to_hash(ctx, inst, sort, true)
        }
        RuntimeCallTarget::MixedCellClone => lower_mixed_cell_clone(ctx, inst),
        RuntimeCallTarget::ArrayUnpackToHash => lower_array_unpack_to_hash(ctx, inst),
        RuntimeCallTarget::ExceptionGuardOwned => super::exception_instructions::lower_guard_owned(ctx, inst),
        RuntimeCallTarget::ExceptionUnguardOwned => super::exception_instructions::lower_unguard_owned(ctx, inst),
        RuntimeCallTarget::ExceptionUpdateArrayGuard | RuntimeCallTarget::ExceptionUpdateHashGuard =>
            super::exception_instructions::lower_update_array_guard(ctx, inst),
        RuntimeCallTarget::CallArgumentValidateUnpack
        | RuntimeCallTarget::CallArgumentCollectPositionals
        | RuntimeCallTarget::CallArgumentCollectNamed => super::call_argument_unpack::lower(ctx, inst, target),
        RuntimeCallTarget::UnaryString(runtime) => lower_unary_string(ctx, inst, runtime),
        RuntimeCallTarget::Pcntl(target) => {
            crate::codegen::lower_inst::builtins::pcntl::lower(ctx, inst, target)
        }
        RuntimeCallTarget::Function(target) => super::runtime_functions::lower(ctx, inst, target),
        RuntimeCallTarget::ProfiledFunction { target, .. } => {
            super::runtime_functions::lower(ctx, inst, target)
        }
    }
}

/// Promotes an independent cell copy and transfers one hash owner to a literal unpack operation.
fn lower_array_unpack_to_hash(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let source = expect_operand(inst, 0)?;
    ctx.load_value_to_result(source)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_reg_move(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 0), result);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cell_promote_to_hash");
    // The promotion lends its payload. Acquire a result owner before retiring the clone,
    // including the invalid-value path where the returned null pointer needs no retain.
    abi::emit_push_reg(ctx.emitter, result);
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_load_temporary_stack_slot(ctx.emitter, result, 16);
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(ctx.emitter, result);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    let valid = ctx.next_label("array_unpack_hash_valid");
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &valid);
    super::exceptions::emit_error(ctx, "Only arrays and Traversables can be unpacked");
    ctx.emitter.label(&valid);
    store_if_result(ctx, inst)
}

/// Materializes normalized constructor parameters through the shared target-aware call ABI.
fn lower_throwable_initialize(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 4 {
        return Err(CodegenIrError::invalid_module("Throwable initialization requires four operands"));
    }
    let types = [PhpType::Object("Throwable".into()), PhpType::Str, PhpType::Int, PhpType::Mixed];
    let overflow = super::materialize_direct_call_args(ctx, &inst.operands, &types)?;
    let padding = super::direct_call_stack_pad_bytes(ctx, overflow);
    abi::emit_reserve_temporary_stack(ctx.emitter, padding);
    abi::emit_call_label(ctx.emitter, "__rt_throwable_initialize");
    abi::emit_release_temporary_stack(ctx.emitter, padding + overflow);
    Ok(())
}

/// Clones a stored Mixed cell before a nested mutation publishes a new payload.
///
/// A shallow COW clone of an array or hash keeps its boxed Mixed slots shared. This operation
/// preserves scalar tags exactly and delegates tag-4/tag-5 payload retention to
/// `__rt_mixed_from_value`, so the returned cell may be safely promoted and installed only in the
/// mutating parent. A null cell remains null for the caller's existing TypeError guard.
fn lower_mixed_cell_clone(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_clone expected 1 operand, got {}",
            inst.operands.len(),
        )));
    }
    let cell = expect_operand(inst, 0)?;
    let actual = ctx.load_value_to_result(cell)?.codegen_repr();
    if actual != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_clone expected Mixed, got {:?}",
            actual,
        )));
    }
    let done = ctx.next_label("mixed_cell_clone_done");
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", done));              // absent cells stay absent so the following promotion raises the normal TypeError
            ctx.emitter.instruction("ldr x2, [x0, #16]");                       // load the copied Mixed high payload before reusing x0 for the tag
            ctx.emitter.instruction("ldr x1, [x0, #8]");                        // load the copied Mixed low payload for the retaining box helper
            ctx.emitter.instruction("ldr x0, [x0]");                            // pass the original runtime tag to the retaining box helper
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // absent cells stay absent so the following promotion raises the normal TypeError
            ctx.emitter.instruction(&format!("jz {}", done));                   // bypass payload loads when no boxed cell was stored
            ctx.emitter.instruction("mov rsi, QWORD PTR [rax + 16]");           // load the copied Mixed high payload before reusing rax for the tag
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // load the copied Mixed low payload for the retaining box helper
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // pass the original runtime tag to the retaining box helper
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Promotes or borrows the array payload of a boxed Mixed cell for a nested key sort.
///
/// The helper mutates tag-4 cells in place, borrows tag-5 payloads unchanged, and returns zero
/// for a null/scalar/missing cell. Its valid hash result remains borrowed from the cell, so EIR
/// ownership stays with the parent storage rather than treating it as freshly owned.
fn lower_mixed_cell_promote_to_hash(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    sort: crate::ir::ArrayKeySort,
    attached: bool,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_promote_to_hash expected 1 operand, got {}",
            inst.operands.len(),
        )));
    }
    let cell = expect_operand(inst, 0)?;
    let actual = ctx.load_value_to_result(cell)?.codegen_repr();
    if actual != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime array.mixed_cell_promote_to_hash expected Mixed, got {:?}",
            actual,
        )));
    }
    if attached {
        separate_shared_attached_cell(ctx, cell)?;
        ctx.load_value_to_result(cell)?;
    }
    if ctx.emitter.target.arch == crate::codegen::platform::Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the boxed Mixed cell in the SysV first-argument register
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_cell_promote_to_hash");
    let valid = ctx.next_label("mixed_cell_promote_to_hash_valid");
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbnz x0, {}", valid));            // nonzero helper results are valid borrowed hash payloads
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // distinguish an invalid Mixed receiver from a hash payload
            ctx.emitter.instruction(&format!("jnz {}", valid));                 // nonzero helper results are valid borrowed hash payloads
        }
    }
    super::exceptions::emit_type_error(
        ctx,
        &format!(
            "{}(): Argument #1 ($array) must be of type array, non-array value given",
            sort.php_name()
        ),
    );
    ctx.emitter.label(&valid);
    store_if_result(ctx, inst)
}

/// Copy-on-write separates an attached Mixed cell that another variable still shares.
///
/// The promotion mutates its cell in place: it republishes the unique, key-sorted hash into the
/// cell's payload word. `$copy = $array;` shares the boxed zval itself, so an in-place promotion
/// reordered the copy too — `k($m)` with `function k(array &$a) { ksort($a); }` sorted `$ma` as
/// well. A sole owner keeps mutating in place, so an unshared sort still costs nothing.
///
/// Only a receiver resolvable to a writable slot can be separated; anything else keeps the
/// existing in-place behaviour.
fn separate_shared_attached_cell(ctx: &mut FunctionContext<'_>, cell: crate::ir::ValueId) -> Result<()> {
    let receiver = ReceiverPlace::resolve(ctx, cell)?;
    // A raw local slot publishes without retiring what it held, so this has to release the
    // replaced cell itself; the ref-cell write-back already retires its previous occupant.
    let retires_replaced_cell = match receiver {
        // A global-backed receiver's write-back retires the symbol's previous cell itself.
        ReceiverPlace::RefCell(_) | ReceiverPlace::Global { .. } => true,
        ReceiverPlace::Local(_) => false,
        ReceiverPlace::Opaque | ReceiverPlace::Property { .. } => return Ok(()),
    };
    let done = ctx.next_label("mixed_cell_attached_separate_done");
    ctx.load_value_to_result(cell)?;
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {done}"));                // an absent cell has nothing to separate
            ctx.emitter.instruction("ldr w9, [x0, #-12]");                      // read the cell refcount from the uniform heap header
            ctx.emitter.instruction("cmp w9, #1");                              // is this zval shared with another variable?
            ctx.emitter.instruction(&format!("b.ls {done}"));                   // a sole owner may be promoted in place
            abi::emit_push_reg(ctx.emitter, "x0");                              // keep the replaced cell addressable for its release
            ctx.emitter.instruction("ldr x2, [x0, #16]");                       // copy the shared cell high payload word
            ctx.emitter.instruction("ldr x1, [x0, #8]");                        // copy the shared cell low payload word
            ctx.emitter.instruction("ldr x0, [x0]");                            // copy the shared cell runtime value tag
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // an absent cell has nothing to separate
            ctx.emitter.instruction(&format!("jz {done}"));                     // keep the promotion on its ordinary path
            ctx.emitter.instruction("mov r10d, DWORD PTR [rax - 12]");          // read the cell refcount from the uniform heap header
            ctx.emitter.instruction("cmp r10d, 1");                             // is this zval shared with another variable?
            ctx.emitter.instruction(&format!("jbe {done}"));                    // a sole owner may be promoted in place
            abi::emit_push_reg(ctx.emitter, "rax");                             // keep the replaced cell addressable for its release
            ctx.emitter.instruction("mov rsi, QWORD PTR [rax + 16]");           // copy the shared cell high payload word
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // copy the shared cell low payload word
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // copy the shared cell runtime value tag
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.store_result_value(cell)?;
    receiver.store_back_value(ctx, cell)?;
    if !retires_replaced_cell {
        abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    } else {
        abi::emit_release_temporary_stack(ctx.emitter, 16);
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers a typed `Str -> Str` transform using the internal string result register pair.
fn lower_unary_string(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    runtime: UnaryStringRuntime,
) -> Result<()> {
    if inst.operands.len() != 1 {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime {} expected 1 operand, got {}",
            runtime.as_eir(),
            inst.operands.len(),
        )));
    }
    let value = expect_operand(inst, 0)?;
    let actual = ctx.load_value_to_result(value)?.codegen_repr();
    if actual != PhpType::Str {
        return Err(CodegenIrError::invalid_module(format!(
            "typed runtime {} expected Str, got {:?}",
            runtime.as_eir(),
            actual,
        )));
    }
    abi::emit_call_label(ctx.emitter, unary_string_symbol(runtime));
    if runtime == UnaryStringRuntime::BinToHex {
        let owned = ctx.next_label("bin2hex_heap_result_owned");
        let ready = ctx.next_label("bin2hex_result_ready");
        let (pointer, length) = abi::string_result_regs(ctx.emitter);
        abi::emit_push_reg_pair(ctx.emitter, pointer, length);
        abi::emit_call_label(ctx.emitter, "__rt_heap_kind");
        let result = abi::int_result_reg(ctx.emitter);
        ctx.emitter.instruction(&format!("cmp {result}, 1"));                   // reserve returns a fresh owned string only after leaving scratch storage
        ctx.emitter.instruction(&format!("{} {owned}", if ctx.emitter.target.arch == crate::codegen::platform::Arch::AArch64 { "b.eq" } else { "je" })); // preserve an already-owned heap result
        abi::emit_pop_reg_pair(ctx.emitter, pointer, length);
        abi::emit_call_label(ctx.emitter, "__rt_str_persist");                 // copy the scratch result before later string operations reuse it
        abi::emit_jump(ctx.emitter, &ready);
        ctx.emitter.label(&owned);
        abi::emit_pop_reg_pair(ctx.emitter, pointer, length);
        ctx.emitter.label(&ready);
    }
    store_if_result(ctx, inst)
}

/// Maps a backend-neutral unary string operation to its concrete runtime symbol.
fn unary_string_symbol(runtime: UnaryStringRuntime) -> &'static str {
    match runtime {
        UnaryStringRuntime::AddSlashes => "__rt_addslashes",
        UnaryStringRuntime::Base64Encode => "__rt_base64_encode",
        UnaryStringRuntime::BinToHex => "__rt_bin2hex",
        UnaryStringRuntime::HexToBin => "__rt_hex2bin",
        UnaryStringRuntime::HtmlEntityDecode => "__rt_html_entity_decode",
        UnaryStringRuntime::NlToBr => "__rt_nl2br",
        UnaryStringRuntime::QuoteMeta => "__rt_quotemeta",
        UnaryStringRuntime::QuotedPrintableEncode => "__rt_quoted_printable_encode",
        UnaryStringRuntime::RawUrlDecode => "__rt_urldecode",
        UnaryStringRuntime::RawUrlEncode => "__rt_rawurlencode",
        UnaryStringRuntime::StripSlashes => "__rt_stripslashes",
        UnaryStringRuntime::StrReverse => "__rt_strrev",
        UnaryStringRuntime::StrToLower => "__rt_strtolower",
        UnaryStringRuntime::StrToUpper => "__rt_strtoupper",
        UnaryStringRuntime::UrlDecode => "__rt_urldecode",
        UnaryStringRuntime::UrlEncode => "__rt_urlencode",
    }
}
