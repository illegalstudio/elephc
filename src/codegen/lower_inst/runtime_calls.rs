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

use super::{expect_operand, store_if_result};

/// Tells the monitor that this call is a STREAM operation, naming it.
///
/// One place rather than one per builtin: every typed runtime call arrives here, so the
/// counter cannot fall behind a builtin somebody adds later — the only thing that decides
/// is `RuntimeFnId::is_stream_operation`, which is the list.
///
/// Emitted ONLY under `--instrument`. The slot it would read is zero in every other binary,
/// so the guarded call would be inert — but "inert" still costs a load, a branch and the
/// instructions around them at every read in every loop, and a build nobody asked to profile
/// should not carry that. This is the same pay-for-use rule the profiler's own enter/exit
/// hooks follow.
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
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("elephc_instr_stream_fn");
    let skip = ctx.next_label("instr_stream_skip");
    let slot = abi::symbol_scratch_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, slot, &symbol, 0);
    match ctx.emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz {}, {}", slot, skip)); // dormant: the slot is zero
            abi::emit_symbol_address(ctx.emitter, "x0", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x1", len as i64);
            ctx.emitter.instruction(&format!("blr {}", slot));           // elephc_instr_stream(name, len)
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", slot, slot));
            ctx.emitter.instruction(&format!("jz {}", skip));            // dormant: the slot is zero
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
            ctx.emitter.instruction(&format!("call {}", slot));          // elephc_instr_stream(name, len)
        }
    }
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
        RuntimeCallTarget::ArrayFetchForWrite => {
            super::lower_array_fetch_for_write_runtime_call(ctx, inst)
        }
        RuntimeCallTarget::MixedCellPromoteToHash(sort)
        | RuntimeCallTarget::MixedCellPromoteAttachedToHash(sort) => {
            lower_mixed_cell_promote_to_hash(ctx, inst, sort)
        }
        RuntimeCallTarget::MixedCellClone => lower_mixed_cell_clone(ctx, inst),
        RuntimeCallTarget::UnaryString(runtime) => lower_unary_string(ctx, inst, runtime),
        RuntimeCallTarget::Function(target) => super::runtime_functions::lower(ctx, inst, target),
        RuntimeCallTarget::ProfiledFunction { target, .. } => {
            super::runtime_functions::lower(ctx, inst, target)
        }
    }
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
