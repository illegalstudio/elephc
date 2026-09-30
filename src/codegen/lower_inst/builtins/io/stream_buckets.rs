//! Purpose:
//! Stream bucket construction and brigade mutation.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Lowers `stream_bucket_new(stream, data)` into a stdClass-backed bucket object.
pub(crate) fn lower_stream_bucket_new(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_bucket_new", 2)?;
    let stream = expect_operand(inst, 0)?;
    let data_value = expect_operand(inst, 1)?;
    ctx.load_value_to_result(stream)?;
    load_string_to_result(ctx, data_value, "stream_bucket_new buffer")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_stream_bucket_new_aarch64(ctx),
        Arch::X86_64 => lower_stream_bucket_new_x86_64(ctx),
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_bucket_make_writeable(brigade)` by popping the brigade head.
pub(crate) fn lower_stream_bucket_make_writeable(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_bucket_make_writeable", 1)?;
    let brigade = expect_operand(inst, 0)?;
    let arg_ty = ctx.load_value_to_result(brigade)?;
    let arg_is_mixed = matches!(arg_ty, PhpType::Mixed | PhpType::Union(_));
    let (buckets_sym, buckets_len) = ctx.data.add_string(b"_buckets");
    let return_null = ctx.next_label("sbmw_null");
    let done = ctx.next_label("sbmw_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if arg_is_mixed {
                ctx.emitter.instruction(&format!("cbz x0, {}", return_null));   // null Mixed means there is no brigade object
                ctx.emitter.instruction("ldr x9, [x0]");                        // load the Mixed runtime tag
                ctx.emitter.instruction("cmp x9, #6");                          // tag 6 identifies object values
                ctx.emitter.instruction(&format!("b.ne {}", return_null));      // non-object brigades are empty
                ctx.emitter.instruction("ldr x0, [x0, #8]");                    // unbox the stdClass object pointer
            }
            ctx.emitter.instruction(&format!("cbz x0, {}", return_null));       // missing brigade object returns null
            abi::emit_symbol_address(ctx.emitter, "x1", &buckets_sym);
            ctx.emitter.instruction(&format!("mov x2, #{}", buckets_len));      // pass the `_buckets` property-name length
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
            ctx.emitter.instruction(&format!("cbz x0, {}", return_null));       // missing `_buckets` property returns null
            ctx.emitter.instruction("ldr x9, [x0]");                            // load the property Mixed tag
            ctx.emitter.instruction("cmp x9, #4");                              // tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("b.ne {}", return_null));          // non-array `_buckets` is treated as empty
            ctx.emitter.instruction("ldr x9, [x0, #8]");                        // unbox the indexed-array pointer
            ctx.emitter.instruction(&format!("cbz x9, {}", return_null));       // null array payload returns null
            ctx.emitter.instruction("ldr x10, [x9]");                           // load the indexed-array length
            ctx.emitter.instruction(&format!("cbz x10, {}", return_null));      // an empty brigade returns null
            ctx.emitter.instruction("mov x0, x9");                              // pass the array pointer to array_shift
            abi::emit_call_label(ctx.emitter, "__rt_array_shift");
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the null-result path
            ctx.emitter.label(&return_null);
            emit_null_mixed(ctx);
            ctx.emitter.label(&done);
        }
        Arch::X86_64 => {
            if arg_is_mixed {
                ctx.emitter.instruction("test rax, rax");                       // null Mixed means there is no brigade object
                ctx.emitter.instruction(&format!("jz {}", return_null));        // branch to the PHP null result
                ctx.emitter.instruction("mov r10, QWORD PTR [rax]");            // load the Mixed runtime tag
                ctx.emitter.instruction("cmp r10, 6");                          // tag 6 identifies object values
                ctx.emitter.instruction(&format!("jne {}", return_null));       // non-object brigades are empty
                ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");        // unbox the stdClass object pointer
            }
            ctx.emitter.instruction("test rax, rax");                           // missing brigade object returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov rdi, rax");                            // pass the brigade object to stdClass lookup
            abi::emit_symbol_address(ctx.emitter, "rsi", &buckets_sym);
            ctx.emitter.instruction(&format!("mov rdx, {}", buckets_len));      // pass the `_buckets` property-name length
            abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
            ctx.emitter.instruction("test rax, rax");                           // missing `_buckets` property returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                // load the property Mixed tag
            ctx.emitter.instruction("cmp r10, 4");                              // tag 4 identifies indexed arrays
            ctx.emitter.instruction(&format!("jne {}", return_null));           // non-array `_buckets` is treated as empty
            ctx.emitter.instruction("mov r10, QWORD PTR [rax + 8]");            // unbox the indexed-array pointer
            ctx.emitter.instruction("test r10, r10");                           // null array payload returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // load the indexed-array length
            ctx.emitter.instruction("test r11, r11");                           // an empty brigade returns null
            ctx.emitter.instruction(&format!("jz {}", return_null));            // branch to the PHP null result
            ctx.emitter.instruction("mov rdi, r10");                            // pass the array pointer to array_shift
            abi::emit_call_label(ctx.emitter, "__rt_array_shift");
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the null-result path
            ctx.emitter.label(&return_null);
            emit_null_mixed(ctx);
            ctx.emitter.label(&done);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_bucket_append()` by adding the bucket at the brigade tail.
pub(crate) fn lower_stream_bucket_append(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stream_bucket_insert(ctx, inst, false)
}

/// Lowers `stream_bucket_prepend()` by adding the bucket at the brigade head.
pub(crate) fn lower_stream_bucket_prepend(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_stream_bucket_insert(ctx, inst, true)
}

/// Lowers append/prepend insertion over a brigade object's `_buckets` array.
pub(super) fn lower_stream_bucket_insert(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_bucket_append/prepend", 2)?;
    let brigade = expect_operand(inst, 0)?;
    let bucket = expect_operand(inst, 1)?;
    let brigade_ty = ctx.load_value_to_result(brigade)?;
    let brigade_is_mixed = matches!(brigade_ty, PhpType::Mixed | PhpType::Union(_));
    let (buckets_sym, buckets_len) = ctx.data.add_string(b"_buckets");
    let done = ctx.next_label("sba_done");
    let init = ctx.next_label("sba_init");
    let existing = ctx.next_label("sba_existing");
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_stream_bucket_insert_aarch64(
            ctx,
            bucket,
            brigade_is_mixed,
            prepend,
            &buckets_sym,
            buckets_len,
            &done,
            &init,
            &existing,
        )?,
        Arch::X86_64 => lower_stream_bucket_insert_x86_64(
            ctx,
            bucket,
            brigade_is_mixed,
            prepend,
            &buckets_sym,
            buckets_len,
            &done,
            &init,
            &existing,
        )?,
    }
    store_if_result(ctx, inst)
}

/// The prefixes php-src resolves to a non-local wrapper. `data:` carries no slashes
/// because the RFC 2397 wrapper is registered for the bare scheme too.
const NON_LOCAL_PATH_PREFIXES: [&str; 5] = ["http://", "https://", "ftp://", "ftps://", "data:"];

/// Answers `stream_is_local()` for a path whose bytes are known at compile time.
///
/// Kept beside the runtime classifier in `stream_is_local.rs`, which applies the same
/// rule to a path that only exists at run time; the two must agree.
fn const_path_is_local(path: &str) -> bool {
    !NON_LOCAL_PATH_PREFIXES
        .iter()
        .any(|prefix| path.len() >= prefix.len() && path[..prefix.len()].eq_ignore_ascii_case(prefix))
}

/// Lowers `stream_is_local(stream_or_path)`, which accepts a stream resource or a path.
pub(crate) fn lower_stream_is_local(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_is_local", 1)?;
    let stream = expect_operand(inst, 0)?;
    // If the arg is a string literal, check the scheme prefix at compile time.
    if let Some(path) = optional_const_string_operand(ctx, stream)? {
        emit_bool_result(ctx, const_path_is_local(&path));
        return store_if_result(ctx, inst);
    }
    // A path that only exists at run time — an array element, a loop variable — goes to the
    // runtime classifier, which applies the same rule to the bytes.
    if matches!(ctx.value_php_type(stream)?, PhpType::Str) {
        load_string_to_result(ctx, stream, "stream_is_local path")?;
        abi::emit_call_label(ctx.emitter, "__rt_stream_is_local_path");
        return store_if_result(ctx, inst);
    }
    // For a resource argument, consume the URL identity frozen into its StreamState.
    load_open_stream_handle_to_result(ctx, stream, "stream_is_local")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            ctx.emitter.instruction(&format!(
                "ldr x9, [x0, #{}]", STREAM_OWNERSHIP_FLAGS_OFFSET
            ));                                                                 // load instance-local stream flags
            ctx.emitter.instruction(&format!(
                "tst x9, #{}", STREAM_STATE_FLAG_IS_URL
            ));                                                                 // does this stream instance use a URL wrapper?
            ctx.emitter.instruction("cset x0, eq");                             // URL streams are non-local; all other streams are local
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // pass the opaque stream handle to the state resolver
            abi::emit_call_label(ctx.emitter, "__rt_stream_state");
            ctx.emitter.instruction(&format!(
                "mov rax, QWORD PTR [rax + {}]", STREAM_OWNERSHIP_FLAGS_OFFSET
            ));                                                                 // load instance-local stream flags
            ctx.emitter.instruction(&format!(
                "test rax, {}", STREAM_STATE_FLAG_IS_URL
            ));                                                                 // does this stream instance use a URL wrapper?
            ctx.emitter.instruction("sete al");                                 // URL streams are non-local; all other streams are local
            ctx.emitter.instruction("movzx eax, al");                           // normalize the predicate to the PHP boolean ABI
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `stream_supports_lock(stream)` as true after resource unboxing.
pub(crate) fn lower_stream_supports_lock(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_supports_lock", 1)?;
    let stream = expect_operand(inst, 0)?;
    // Not every stream locks: php-src answers from the stream's ops, and the memory and
    // output wrappers carry no lock option. Answering a blanket true told a caller that
    // `flock()` on `php://memory` would serialise anything.
    load_open_stream_handle_to_result(ctx, stream, "stream_supports_lock")?;
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the opaque stream handle
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_supports_lock");
    store_if_result(ctx, inst)
}

