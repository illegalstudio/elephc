//! Purpose:
//! Builtin, compressed, iconv, and user stream filter attachment.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;
use crate::codegen_support::runtime::resources::layout::{
    FILTER_FLAGS_OFFSET, FILTER_FLAG_INERT, FILTER_STREAM_HANDLE_OFFSET,
};

/// php-src's verbatim `ValueError` for an empty `stream_filter_register()` `$filter_name`.
const STREAM_FILTER_REGISTER_EMPTY_NAME_MESSAGE: &str =
    "stream_filter_register(): Argument #1 ($filter_name) must be a non-empty string";

/// php-src's verbatim `ValueError` for an empty `stream_filter_register()` `$class`.
const STREAM_FILTER_REGISTER_EMPTY_CLASS_MESSAGE: &str =
    "stream_filter_register(): Argument #2 ($class) must be a non-empty string";

/// Lowers `stream_filter_register(filter_name, class)` into the user-filter registry helper.
pub(crate) fn lower_stream_filter_register(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_filter_register", 2)?;
    let filter_name = expect_operand(inst, 0)?;
    let class_name = expect_operand(inst, 1)?;
    load_string_to_result(ctx, filter_name, "stream_filter_register filter_name")?;
    // php rejects an empty name or class outright, before it looks at the registry at all.
    // Both are `ValueError`s the caller can catch, so they are raised on the byte length rather
    // than refused at compile time — the argument is a `string`, not a literal, in general.
    let (_, name_len_reg) = abi::string_result_regs(ctx.emitter);
    super::super::exceptions::emit_value_error_unless(
        ctx,
        super::super::exceptions::ValueGuard::SignedAtLeast(name_len_reg, 1),
        STREAM_FILTER_REGISTER_EMPTY_NAME_MESSAGE,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            load_string_to_result(ctx, class_name, "stream_filter_register class")?;
            super::super::exceptions::emit_value_error_unless(
                ctx,
                super::super::exceptions::ValueGuard::SignedAtLeast("x2", 1),
                STREAM_FILTER_REGISTER_EMPTY_CLASS_MESSAGE,
            );
            ctx.emitter.instruction("mov x3, x2");                              // pass the class-name byte length as the fourth registry argument
            ctx.emitter.instruction("mov x2, x1");                              // pass the class-name pointer as the third registry argument
            abi::emit_pop_reg_pair(ctx.emitter, "x0", "x1");
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            load_string_to_result(ctx, class_name, "stream_filter_register class")?;
            super::super::exceptions::emit_value_error_unless(
                ctx,
                super::super::exceptions::ValueGuard::SignedAtLeast("rdx", 1),
                STREAM_FILTER_REGISTER_EMPTY_CLASS_MESSAGE,
            );
            ctx.emitter.instruction("mov rcx, rdx");                            // pass the class-name byte length as the fourth registry argument
            ctx.emitter.instruction("mov rdx, rax");                            // pass the class-name pointer as the third registry argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_stream_filter_register");
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_append` and `stream_filter_prepend`.
///
/// A mode naming BOTH directions makes TWO filters, one per chain — see `emit_attach_filter_node`,
/// which every attach below reaches. php's `apply_filter_to_stream` creates the read one, then the
/// write one, and returns the write one, so `stream_filter_remove()` on the returned resource
/// leaves the read side filtering.
///
/// ⚠️ A USER filter still gets ONE instance where php makes two: php runs `onCreate()` once per
/// direction, and the instance is minted by `__rt_stream_filter_attach_user` before the node
/// exists, so the second one has to come from the attach path rather than from the node creator.
pub(crate) fn lower_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 2, 4)?;
    // Ordering is the chain's job now: prepend inserts at the head instead of
    // shuffling two fixed table slots, so a third filter no longer falls off.
    let prepend = name == "stream_filter_prepend";
    let filter = expect_operand(inst, 1)?;
    if let Some(filter_name) = optional_const_string_operand(ctx, filter)? {
        if filter_name == "zlib.deflate" {
            return lower_zlib_deflate_stream_filter_attach(ctx, inst);
        }
        if filter_name == "zlib.inflate" {
            return lower_zlib_inflate_stream_filter_attach(ctx, inst);
        }
        if filter_name == "bzip2.compress" {
            return lower_bzip2_compress_stream_filter_attach(ctx, inst);
        }
        if filter_name == "bzip2.decompress" {
            return lower_bzip2_decompress_stream_filter_attach(ctx, inst);
        }
        if let Some(spec) = filter_name.strip_prefix("convert.iconv.") {
            // A name with no separator is not a filter at all: `convert.iconv.` and
            // `convert.iconv.UTF-8` both answer `false` on `php -n` 8.5.6, and this attached an
            // inert node and reported success for both. An EMPTY half is a different thing and
            // stays supported — `convert.iconv.UTF-8/` attaches, iconv reading the empty string
            // as the current locale's charset.
            if !spec.contains('/') {
                return lower_refused_iconv_filter_attach(ctx, inst, prepend);
            }
            return lower_iconv_stream_filter_attach(ctx, inst, spec, prepend);
        }
        if let Some(id) = stream_filter_id(&filter_name) {
            return lower_builtin_stream_filter_attach(ctx, inst, id, prepend);
        }
        return lower_user_stream_filter_attach(ctx, inst, prepend);
    }
    lower_dynamic_inline_shape_filter_attach(ctx, inst, prepend)
}

/// The inline-shape filters, paired with the attach each one needs.
///
/// These four cannot be reached through `__rt_builtin_filter_id`, the run-time name table the
/// dynamic path scans. That table lists what a CHAIN NODE can apply — a byte transform the id
/// selects — and these are not that: each installs a per-fd handle plus a program-local helper
/// thunk (`_zlib_fwrite_fn` and friends) that only a compile-time attach sequence can emit.
/// Naming them in the table would mint a node whose id the chain hands to a byte transform that
/// does not exist, so the attach would report success and filter nothing.
///
/// The way to reach them from a run-time name is therefore to emit the attach sequences AT THE
/// CALL SITE and pick between them with a name comparison, which is what this table drives.
const DYNAMIC_INLINE_SHAPE_FILTERS: &[(
    &str,
    fn(&mut FunctionContext<'_>, &Instruction) -> Result<()>,
)] = &[
    ("zlib.deflate", lower_zlib_deflate_stream_filter_attach),
    ("zlib.inflate", lower_zlib_inflate_stream_filter_attach),
    ("bzip2.compress", lower_bzip2_compress_stream_filter_attach),
    (
        "bzip2.decompress",
        lower_bzip2_decompress_stream_filter_attach,
    ),
];

/// Lowers a `stream_filter_append()` / `stream_filter_prepend()` whose `$filter_name` is not a
/// compile-time literal.
///
/// `$name = "zlib.deflate"; stream_filter_append($h, $name);` attached NOTHING and answered
/// `false`, while the same call with the literal compresses — php makes no such distinction, and
/// a filter name held in a variable is ordinary PHP (a config value, a loop over a list). The
/// four names above are compared here in turn, each branch emitting the very sequence the
/// literal path emits, and anything else falls through to the user/table path unchanged.
fn lower_dynamic_inline_shape_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    let filter = expect_operand(inst, 1)?;
    let done_label = ctx.next_label("filter_dyn_done");
    for (name, attach) in DYNAMIC_INLINE_SHAPE_FILTERS {
        let next_label = ctx.next_label("filter_dyn_next");
        emit_filter_name_mismatch_branch(ctx, filter, name, &next_label)?;
        attach(ctx, inst)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done_label)),
            Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done_label)),
        }
        ctx.emitter.label(&next_label);
    }
    lower_dynamic_iconv_filter_attach(ctx, inst, filter, prepend, &done_label)?;
    lower_user_stream_filter_attach(ctx, inst, prepend)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the `convert.iconv.<from>/<to>` arm of the dynamic-name attach.
///
/// The other four names are fixed strings a comparison can match. This one carries its two
/// charsets INSIDE the name, and the attach sequence bakes them into `iconv_open()` as data
/// symbols — which a name that only exists at run time cannot supply. So the same sequence is
/// emitted against two program-local buffers, and `__rt_iconv_spec_split` fills them just
/// before it runs. The shapes are untouched: they already take the ADDRESS of a symbol.
///
/// A name with no `/` after the prefix falls through to the caller's user-filter path, which is
/// what php does — `convert.iconv.` and `convert.iconv.UTF-8` both answer `false` on `php -n`
/// 8.5.6, while `convert.iconv.UTF-8/` and `convert.iconv./UTF-8` attach, an empty half being
/// iconv's own way of naming the current locale's charset.
fn lower_dynamic_iconv_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    filter: ValueId,
    prepend: bool,
    done_label: &str,
) -> Result<()> {
    use crate::codegen_support::runtime::io::ICONV_SPEC_BUFFER_BYTES;

    let next_label = ctx.next_label("filter_dyn_next");
    // `next_global_label`, not `next_label`: these two name `.comm` BUFFERS, and a `.comm` needs a
    // real symbol. The assembler-local prefix made the directive itself invalid — `non-local symbol
    // required` — and then left every `adrp`/`lea` that took their address pointing at a label
    // nothing defined. Six `io::streams` filter tests did not ASSEMBLE, and the failure named the
    // address-taking line rather than the directive that caused it. `next_global_label`'s own doc
    // names this error.
    let from_symbol = ctx.next_global_label("iconv_dyn_from");
    let to_symbol = ctx.next_global_label("iconv_dyn_to");
    ctx.data
        .add_comm(from_symbol.clone(), ICONV_SPEC_BUFFER_BYTES);
    ctx.data.add_comm(to_symbol.clone(), ICONV_SPEC_BUFFER_BYTES);

    load_string_to_result(ctx, filter, "stream_filter_append name")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // the run-time name pointer
            ctx.emitter.instruction("mov x1, x2");                              // and its byte length
            abi::emit_symbol_address(ctx.emitter, "x2", &from_symbol);
            abi::emit_symbol_address(ctx.emitter, "x3", &to_symbol);
            abi::emit_call_label(ctx.emitter, "__rt_iconv_spec_split");
            ctx.emitter
                .instruction(&format!("cbz x0, {}", next_label));               // not an iconv name: try the rest
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // the run-time name pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // and its byte length
            ctx.emitter
                .instruction(&format!("lea rdx, [rip + {from_symbol}]"));
            ctx.emitter
                .instruction(&format!("lea rcx, [rip + {to_symbol}]"));
            abi::emit_call_label(ctx.emitter, "__rt_iconv_spec_split");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter
                .instruction(&format!("je {}", next_label));                    // not an iconv name: try the rest
        }
    }

    // The charset pair is validated here exactly as on the literal path: php creates the filter
    // at attach time, so an unopenable conversion answers `false` rather than attaching a
    // transform that silently passes bytes through.
    let refused_label = ctx.next_label("iconv_dyn_refused");
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    emit_iconv_open_probe(ctx, &from_symbol, &to_symbol, &refused_label);
    emit_iconv_transform_for_current_fd(ctx, inst, &from_symbol, &to_symbol)?;
    emit_inert_filter_resource(ctx, inst, false)?;
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);
    ctx.emitter.label(&refused_label);
    emit_iconv_create_warning(ctx, inst, prepend)?;
    emit_boxed_bool(ctx, false);
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, done_label);
    ctx.emitter.label(&next_label);
    Ok(())
}

/// Branches to `mismatch_label` unless the run-time filter name equals `name`.
///
/// The name operand is re-loaded for every candidate rather than held in a register across the
/// chain: an attach sequence in an earlier arm clobbers whatever it liked, and the loads are
/// pointer/length pairs out of a slot, not work worth saving.
fn emit_filter_name_mismatch_branch(
    ctx: &mut FunctionContext<'_>,
    filter: ValueId,
    name: &str,
    mismatch_label: &str,
) -> Result<()> {
    let (symbol, length) = ctx.data.add_string(name.as_bytes());
    load_string_to_result(ctx, filter, "stream_filter_append name")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // `__rt_str_eq` takes x1/x2 = the first pair, which is where the load already left
            // the run-time name, and x3/x4 = the second.
            abi::emit_symbol_address(ctx.emitter, "x3", &symbol);
            ctx.emitter.instruction(&format!("mov x4, #{length}"));
            abi::emit_call_label(ctx.emitter, "__rt_str_eq");
            ctx.emitter
                .instruction(&format!("cbz x0, {}", mismatch_label));
        }
        Arch::X86_64 => {
            // `__rt_str_eq` takes rdi/rsi = the first pair and rdx/rcx = the second; the load
            // leaves the run-time name in rax/rdx, so both move before the literal is placed.
            ctx.emitter.instruction("mov rdi, rax");                            // the run-time name pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // and its byte length
            ctx.emitter
                .instruction(&format!("lea rdx, [rip + {symbol}]"));            // the candidate name pointer
            ctx.emitter.instruction(&format!("mov rcx, {length}"));             // and its byte length
            abi::emit_call_label(ctx.emitter, "__rt_str_eq");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter
                .instruction(&format!("je {}", mismatch_label));
        }
    }
    Ok(())
}

/// Lowers `stream_filter_append($stream, "zlib.deflate", ...)`.
pub(super) fn lower_zlib_deflate_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    let level = const_int_filter_param(ctx, inst, "level", true, -1, 9)?.unwrap_or(-1);
    let fwrite_label = ctx.next_label("zlib_deflate_fwrite");
    let close_label = ctx.next_label("zlib_deflate_close");
    let skip_label = ctx.next_label("zlib_deflate_skip_helpers");
    let flush_label = ctx.next_label("zlib_deflate_flush");
    let shape = crate::codegen::stream_filters::zlib::filter_shape(
        &fwrite_label,
        &close_label,
        &skip_label,
        &flush_label,
        level,
    );
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::stream_filters::zlib::emit_arm64(ctx.emitter, shape),
        Arch::X86_64 => crate::codegen::stream_filters::zlib::emit_x86_64(ctx.emitter, shape),
    }
    emit_inert_filter_resource(ctx, inst, false)?;
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_append($stream, "zlib.inflate", ...)`.
pub(super) fn lower_zlib_inflate_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    emit_zlib_inflate_attach_in_place(ctx);
    emit_inert_filter_resource(ctx, inst, false)?;
    store_if_result(ctx, inst)
}

/// Attaches the `zlib.inflate` read filter to the stream descriptor already held
/// in the integer result register, leaving a resource-boxed `Mixed` in that
/// register. Shared by `stream_filter_append("zlib.inflate")` and the
/// `compress.zlib://` fopen wrapper.
pub(super) fn emit_zlib_inflate_attach_in_place(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let labels = vec![
                ctx.next_label("zlib_inflate_slurp"),
                ctx.next_label("zlib_inflate_slurped"),
                ctx.next_label("zlib_inflate_zero"),
                ctx.next_label("zlib_inflate_zeroed"),
                ctx.next_label("zlib_inflate_raw"),
                ctx.next_label("zlib_inflate_write"),
                ctx.next_label("zlib_inflate_written"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::stream_filters::inflate::emit_arm64(ctx.emitter, |_| {
                labels.next().expect("zlib inflate ARM64 label")
            });
        }
        Arch::X86_64 => {
            let labels = vec![
                ctx.next_label("zlib_inflate_slurp"),
                ctx.next_label("zlib_inflate_slurped"),
                ctx.next_label("zlib_inflate_sized"),
                ctx.next_label("zlib_inflate_zero"),
                ctx.next_label("zlib_inflate_zeroed"),
                ctx.next_label("zlib_inflate_raw"),
                ctx.next_label("zlib_inflate_write"),
                ctx.next_label("zlib_inflate_written"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::stream_filters::inflate::emit_x86_64(ctx.emitter, |_| {
                labels.next().expect("zlib inflate x86_64 label")
            });
        }
    }
}

/// Lowers `stream_filter_append($stream, "bzip2.compress", ...)`.
pub(super) fn lower_bzip2_compress_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    let block_size = const_int_filter_param(ctx, inst, "blocks", true, 1, 9)?.unwrap_or(9);
    let work_factor = const_int_filter_param(ctx, inst, "work", false, 0, 250)?.unwrap_or(0);
    let fwrite_label = ctx.next_label("bz2_compress_fwrite");
    let close_label = ctx.next_label("bz2_compress_close");
    let skip_label = ctx.next_label("bz2_compress_skip_helpers");
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::stream_filters::bzip2::emit_compress_arm64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            block_size,
            work_factor,
        ),
        Arch::X86_64 => crate::codegen::stream_filters::bzip2::emit_compress_x86_64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            block_size,
            work_factor,
        ),
    }
    emit_inert_filter_resource(ctx, inst, false)?;
    store_if_result(ctx, inst)
}

/// Lowers `stream_filter_append($stream, "bzip2.decompress", ...)`.
pub(super) fn lower_bzip2_decompress_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let stream = expect_operand(inst, 0)?;
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    emit_bzip2_decompress_attach_in_place(ctx);
    emit_inert_filter_resource(ctx, inst, false)?;
    store_if_result(ctx, inst)
}

/// Attaches the `bzip2.decompress` read filter to the stream descriptor already
/// held in the integer result register, leaving a resource-boxed `Mixed` in that
/// register. Shared by `stream_filter_append("bzip2.decompress")` and the
/// `compress.bzip2://` fopen wrapper.
pub(super) fn emit_bzip2_decompress_attach_in_place(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let labels = vec![
                ctx.next_label("bz2_slurp"),
                ctx.next_label("bz2_slurped"),
                ctx.next_label("bz2_write"),
                ctx.next_label("bz2_written"),
                ctx.next_label("bz2_decompress_fail"),
                ctx.next_label("bz2_done_arm"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::stream_filters::bzip2::emit_decompress_arm64(ctx.emitter, |_| {
                labels.next().expect("bzip2 decompress ARM64 label")
            });
        }
        Arch::X86_64 => {
            let labels = vec![
                ctx.next_label("bz2_slurp_x"),
                ctx.next_label("bz2_slurped_x"),
                ctx.next_label("bz2_write_x"),
                ctx.next_label("bz2_written_x"),
                ctx.next_label("bz2_decompress_fail_x"),
                ctx.next_label("bz2_done_x"),
            ];
            let mut labels = labels.into_iter();
            crate::codegen::stream_filters::bzip2::emit_decompress_x86_64(ctx.emitter, |_| {
                labels.next().expect("bzip2 decompress x86_64 label")
            });
        }
    }
}

/// Selects which read-direction decompressor a `compress.*://` fopen wrapper attaches.
#[derive(Clone, Copy)]
pub(super) enum CompressWrapper {
    Zlib,
    Bzip2,
}

/// Re-adopts a compress-wrapper descriptor into the resource registry.
///
/// The `zlib.inflate` / `bzip2.decompress` attach emitters finish by re-boxing
/// the raw descriptor with `__rt_mixed_from_value(9, fd)` so they can also serve
/// `stream_filter_append`. On the `compress.*://` wrapper path that leaves an
/// unregistered descriptor, which `fclose()` cannot resolve. Unbox it and hand
/// the descriptor to the registry adoption used by every other literal open.
///
/// Input:  x0 / rax = Mixed cell boxing the raw descriptor.
/// Output: x0 / rax = Mixed cell boxing the registry handle, or PHP false.
fn emit_adopt_attached_compress_descriptor(ctx: &mut FunctionContext<'_>) {
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // recover the raw descriptor from the Mixed payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, rdi");                            // recover the raw descriptor from the Mixed payload
        }
    }
    box_stream_fd_or_false_result(ctx, "compress_adopt");
}

/// What a `compress.*://` open does with the mode it was handed.
///
/// php's wrapper looks at the FIRST character only and refuses any `+`. MEASURED on
/// `php -n` 8.5.6: `r`/`rb`/`rt`/`rw` read, `w`/`wb`/`a`/`ab` write, and `r+`/`w+`/`a+`/`x`/`c`
/// all answer `false` — `rw` reads because its first character is `r`, and `c`, which the
/// plain-file wrapper accepts, is refused here.
#[derive(Clone, Copy, PartialEq)]
enum CompressWrapperDirection {
    /// Attach the decompressor: later reads see plain bytes.
    Read,
    /// Attach the compressor: later writes are deflated on the way out.
    Write,
    /// php refuses the mode outright and `fopen()` answers `false`.
    Refused,
}

/// Classifies a `compress.*://` open mode the way php-src's zlib wrapper does.
fn compress_wrapper_direction(mode: &str) -> CompressWrapperDirection {
    if mode.contains('+') {
        return CompressWrapperDirection::Refused;
    }
    match mode.as_bytes().first() {
        Some(b'r') => CompressWrapperDirection::Read,
        Some(b'w') | Some(b'a') => CompressWrapperDirection::Write,
        _ => CompressWrapperDirection::Refused,
    }
}

/// php's default `zlib.level`, and zlib's own `Z_DEFAULT_COMPRESSION`.
const ZLIB_DEFAULT_LEVEL: i64 = -1;
/// The highest level `deflateInit2_` accepts.
const ZLIB_MAX_LEVEL: i64 = 9;

/// Reads `zlib.level` out of the open's stream context and publishes it, clamped.
///
/// php reads this option in `ext/zlib/zlib_fopen_wrapper.c` and hands it straight to
/// `deflateInit2_`. The value only exists as a live hash — `stream_context_create(['zlib' =>
/// ['level' => 9]])` is never inspected at compile time — so the walk happens here, inside the
/// context scope `begin_fopen_context_scope` has already published.
///
/// The clamp is a DELIBERATE divergence, taken for safety rather than fidelity: php passes an
/// out-of-range level through, `deflateInit2_` refuses it, and the resulting stream writes zero
/// bytes (MEASURED: `zlib.level => 12` leaves a 0-byte file and `fwrite()` answers 0). elephc's
/// deflate helpers loop until zlib consumes the input, so a stream whose `state` never
/// initialized would spin forever instead. Clamping keeps the absurd input producing a correct
/// file; every level php actually accepts, -1 through 9, passes through untouched.
fn emit_publish_zlib_wrapper_level(ctx: &mut FunctionContext<'_>) {
    let (wrapper_label, wrapper_len) = ctx.data.add_string(b"zlib");
    let (option_label, option_len) = ctx.data.add_string(b"level");
    let capped_label = ctx.next_label("zlib_level_capped");
    let clamped_label = ctx.next_label("zlib_level_clamped");
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x9", ZLIB_DEFAULT_LEVEL);
            ctx.emitter.instruction("str x9, [sp, #0]");                        // a missing option keeps Z_DEFAULT_COMPRESSION
            abi::emit_symbol_address(ctx.emitter, "x0", &wrapper_label);
            ctx.emitter
                .instruction(&format!("mov x1, #{}", wrapper_len));             // strlen("zlib")
            abi::emit_symbol_address(ctx.emitter, "x2", &option_label);
            ctx.emitter
                .instruction(&format!("mov x3, #{}", option_len));              // strlen("level")
            ctx.emitter.instruction("add x4, sp, #0");                          // out address for the resolved level
            abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
            // The level rides in x11, NOT x9: `emit_store_reg_to_symbol` takes x9 for the
            // symbol address, so storing FROM x9 publishes the ADDRESS instead of the value —
            // which `deflateInit2_` rejects, leaving a stream whose write loop never finishes.
            ctx.emitter.instruction("ldr x11, [sp, #0]");                       // the level the context named, or the default
            ctx.emitter
                .instruction(&format!("cmp x11, #{}", ZLIB_MAX_LEVEL));         // above zlib's ceiling?
            ctx.emitter
                .instruction(&format!("b.le {}", capped_label));
            abi::emit_load_int_immediate(ctx.emitter, "x11", ZLIB_MAX_LEVEL);
            ctx.emitter.label(&capped_label);
            abi::emit_load_int_immediate(ctx.emitter, "x10", ZLIB_DEFAULT_LEVEL);
            ctx.emitter.instruction("cmp x11, x10");                            // below zlib's floor?
            ctx.emitter
                .instruction(&format!("b.ge {}", clamped_label));
            ctx.emitter.instruction("mov x11, x10");                            // clamp back to Z_DEFAULT_COMPRESSION
            ctx.emitter.label(&clamped_label);
            abi::emit_store_reg_to_symbol(ctx.emitter, "x11", "_zlib_wrapper_level", 0);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", ZLIB_DEFAULT_LEVEL);
            ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                // a missing option keeps Z_DEFAULT_COMPRESSION
            abi::emit_symbol_address(ctx.emitter, "rdi", &wrapper_label);
            ctx.emitter
                .instruction(&format!("mov rsi, {}", wrapper_len));             // strlen("zlib")
            abi::emit_symbol_address(ctx.emitter, "rdx", &option_label);
            ctx.emitter
                .instruction(&format!("mov rcx, {}", option_len));              // strlen("level")
            ctx.emitter.instruction("mov r8, rsp");                             // out address for the resolved level
            abi::emit_call_label(ctx.emitter, "__rt_get_int_context_option");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // the level the context named, or the default
            ctx.emitter
                .instruction(&format!("cmp rax, {}", ZLIB_MAX_LEVEL));          // above zlib's ceiling?
            ctx.emitter
                .instruction(&format!("jle {}", capped_label));
            ctx.emitter
                .instruction(&format!("mov rax, {}", ZLIB_MAX_LEVEL));
            ctx.emitter.label(&capped_label);
            ctx.emitter
                .instruction(&format!("cmp rax, {}", ZLIB_DEFAULT_LEVEL));      // below zlib's floor?
            ctx.emitter
                .instruction(&format!("jge {}", clamped_label));
            ctx.emitter
                .instruction(&format!("mov rax, {}", ZLIB_DEFAULT_LEVEL));      // clamp back to Z_DEFAULT_COMPRESSION
            ctx.emitter.label(&clamped_label);
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_zlib_wrapper_level", 0);
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
}

/// Attaches the gzip deflate transform to the descriptor already in the result register.
///
/// php's `compress.zlib://` is `gzopen`-backed, so what lands on disk is a GZIP member — header,
/// deflate body and CRC/ISIZE trailer — not the raw deflate the `zlib.deflate` FILTER writes.
/// Everything else is shared with that filter: the same inline `fwrite`/close helpers, the same
/// per-descriptor `_zstream_handles` slot, and the same `_stream_write_filters` id, so
/// `fwrite()` and `fclose()` need no new dispatch.
pub(super) fn emit_zlib_deflate_wrapper_attach_in_place(ctx: &mut FunctionContext<'_>) {
    let fwrite_label = ctx.next_label("zlib_gz_fwrite");
    let close_label = ctx.next_label("zlib_gz_close");
    let skip_label = ctx.next_label("zlib_gz_skip_helpers");
    let flush_label = ctx.next_label("zlib_wrapper_flush");
    let shape = crate::codegen::stream_filters::zlib::DeflateShape {
        fwrite_label: &fwrite_label,
        close_label: &close_label,
        skip_label: &skip_label,
        level: crate::codegen::stream_filters::zlib::DeflateLevel::Slot("_zlib_wrapper_level"),
        window_bits: 31,
        sync_flush_on_close: true,
        flush_label: &flush_label,
    };
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::stream_filters::zlib::emit_arm64(ctx.emitter, shape),
        Arch::X86_64 => crate::codegen::stream_filters::zlib::emit_x86_64(ctx.emitter, shape),
    }
}

/// Attaches the `bzip2.compress` transform to the descriptor a `compress.bzip2://` write opened.
///
/// The same shape the `bzip2.compress` FILTER installs, with php's wrapper defaults: block size 9
/// and work factor 0. `compress.zlib://` reads its level from the stream context; bzip2 has no
/// such context option in php, so there is nothing to publish and no option walk to spill across.
pub(super) fn emit_bzip2_compress_wrapper_attach_in_place(ctx: &mut FunctionContext<'_>) {
    let fwrite_label = ctx.next_label("bz2_wrapper_fwrite");
    let close_label = ctx.next_label("bz2_wrapper_close");
    let skip_label = ctx.next_label("bz2_wrapper_skip_helpers");
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::stream_filters::bzip2::emit_compress_arm64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            9,
            0,
        ),
        Arch::X86_64 => crate::codegen::stream_filters::bzip2::emit_compress_x86_64(
            ctx.emitter,
            &fwrite_label,
            &close_label,
            &skip_label,
            9,
            0,
        ),
    }
}

/// Opens `underlying` through `__rt_fopen` and attaches the transform its DIRECTION selects,
/// boxing the filtered descriptor as a resource.
///
/// An empty path, a failed open, or a mode php's wrapper refuses all box PHP false. The write
/// direction exists for `compress.zlib://` only: `compress.bzip2://` stays read-only, which is
/// a measured gap and not an oversight.
/// Where the underlying path a `compress.*://` open works on comes from.
///
/// A literal URL is split during lowering and its remainder baked as a data string. A URL only
/// known at RUN time cannot be: the wrapper prefix is stripped from the staged string registers
/// instead, which is the whole difference between the two openers.
#[derive(Clone, Copy)]
pub(super) enum CompressUnderlying<'a> {
    /// The remainder after the wrapper prefix, known at compile time.
    Literal(&'a str),
    /// The FULL url sits in the string result registers; skip this many prefix bytes.
    Staged { prefix_len: usize },
}

pub(super) fn emit_literal_compress_wrapper_fopen_result(
    ctx: &mut FunctionContext<'_>,
    underlying: CompressUnderlying<'_>,
    full_uri: &str,
    kind: CompressWrapper,
    mode: &str,
) -> Result<()> {
    let direction = compress_wrapper_direction(mode);
    let literal_underlying = match underlying {
        CompressUnderlying::Literal(path) => Some(path),
        CompressUnderlying::Staged { .. } => None,
    };
    if literal_underlying.is_some_and(str::is_empty)
        || direction == CompressWrapperDirection::Refused
    {
        emit_fd_result(ctx, -1);
        box_stream_fd_or_false_result(ctx, "fopen");
        return Ok(());
    }
    let writing = direction == CompressWrapperDirection::Write;
    let staged = matches!(underlying, CompressUnderlying::Staged { .. });
    if writing && matches!(kind, CompressWrapper::Zlib) {
        // The level has to be read BEFORE the open: `__rt_fopen` returns the descriptor in the
        // very register the option walk would clobber, and the context scope is live either way.
        //
        // The STAGED form carries the url in those same string registers, so it is spilled across
        // the option walk. Without that the prefix arithmetic below ran on whatever the walk left
        // behind and the program segfaulted — the literal form never noticed, because it
        // materializes its path from a data symbol after the walk has finished.
        if staged {
            match ctx.emitter.target.arch {
                Arch::AArch64 => abi::emit_push_reg_pair(ctx.emitter, "x1", "x2"),
                Arch::X86_64 => abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx"),
            }
        }
        emit_publish_zlib_wrapper_level(ctx);
        if staged {
            match ctx.emitter.target.arch {
                Arch::AArch64 => abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2"),
                Arch::X86_64 => abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx"),
            }
        }
    }
    let open_mode: &[u8] = if !writing {
        b"r"
    } else if mode.starts_with('a') {
        b"a"
    } else {
        b"w"
    };
    let (mode_label, mode_len) = ctx.data.add_string(open_mode);
    // The staged form arrives with the FULL url in the string registers, so the wrapper prefix is
    // skipped by pointer arithmetic where the literal form bakes the remainder as its own string.
    match underlying {
        CompressUnderlying::Literal(path) => {
            let (path_label, path_len) = ctx.data.add_string(path.as_bytes());
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    abi::emit_symbol_address(ctx.emitter, "x1", &path_label);
                    ctx.emitter.instruction(&format!("mov x2, #{}", path_len)); // the underlying path byte length
                }
                Arch::X86_64 => {
                    abi::emit_symbol_address(ctx.emitter, "rax", &path_label);
                    ctx.emitter.instruction(&format!("mov rdx, {}", path_len)); // the underlying path byte length
                }
            }
        }
        CompressUnderlying::Staged { prefix_len } => match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("add x1, x1, #{prefix_len}")); // step past the wrapper prefix
                ctx.emitter.instruction(&format!("sub x2, x2, #{prefix_len}")); // and shorten the length to match
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("add rax, {prefix_len}"));     // step past the wrapper prefix
                ctx.emitter.instruction(&format!("sub rdx, {prefix_len}"));     // and shorten the length to match
            }
        },
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x3", &mode_label);
            ctx.emitter.instruction(&format!("mov x4, #{}", mode_len));         // pass the open-mode string byte length
            abi::emit_call_label(ctx.emitter, "__rt_fopen");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &mode_label);
            ctx.emitter.instruction(&format!("mov rsi, {}", mode_len));         // pass the open-mode string byte length
            abi::emit_call_label(ctx.emitter, "__rt_fopen");
        }
    }
    let false_label = ctx.next_label("compress_fopen_false");
    let done_label = ctx.next_label("compress_fopen_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // negative descriptor means the underlying open failed
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false when the source could not be opened
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // negative descriptor means the underlying open failed
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false when the source could not be opened
        }
    }
    // The attach helpers end by re-boxing through `__rt_mixed_from_value(9, fd)`,
    // which wraps the RAW descriptor. That predates the resource registry: the
    // stream never got adopted, so `fclose()` could not resolve the handle and
    // raised. Unbox the descriptor back out and adopt it properly, matching
    // `emit_runtime_fopen_literal_result`'s box-then-record order.
    match kind {
        CompressWrapper::Zlib => {
            if writing {
                emit_zlib_deflate_wrapper_attach_in_place(ctx);
            } else {
                emit_zlib_inflate_attach_in_place(ctx);
            }
            emit_adopt_attached_compress_descriptor(ctx);
            emit_record_stream_meta_after_boxed_literal(ctx, 8, full_uri);
        }
        CompressWrapper::Bzip2 => {
            if writing {
                emit_bzip2_compress_wrapper_attach_in_place(ctx);
            } else {
                emit_bzip2_decompress_attach_in_place(ctx);
            }
            emit_adopt_attached_compress_descriptor(ctx);
            emit_record_stream_meta_after_boxed_literal(ctx, 9, full_uri);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", done_label)), // skip false boxing after attaching the decompressor
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", done_label)),// skip false boxing after attaching the decompressor
    }
    ctx.emitter.label(&false_label);
    box_stream_fd_or_false_result(ctx, "fopen");
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Lowers `stream_filter_append($stream, "convert.iconv.<from>/<to>", ...)`.
pub(super) fn lower_iconv_stream_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    spec: &str,
    prepend: bool,
) -> Result<()> {
    let (from, to) = spec
        .split_once('/')
        .expect("caller rejects an iconv spec with no separator");
    let from_cstr = format!("{}\0", from);
    let to_cstr = format!("{}\0", to);
    let (from_sym, _) = ctx.data.add_string(from_cstr.as_bytes());
    let (to_sym, _) = ctx.data.add_string(to_cstr.as_bytes());
    let stream = expect_operand(inst, 0)?;
    let refused_label = ctx.next_label("iconv_refused");
    let done_label = ctx.next_label("iconv_attached");
    load_stream_fd_to_result(ctx, stream, "stream_filter_append")?;
    emit_iconv_open_probe(ctx, &from_sym, &to_sym, &refused_label);
    emit_iconv_transform_for_current_fd(ctx, inst, &from_sym, &to_sym)?;
    emit_inert_filter_resource(ctx, inst, false)?;
    store_if_result(ctx, inst)?;
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&refused_label);
    emit_iconv_create_warning(ctx, inst, prepend)?;
    emit_boxed_bool(ctx, false);
    store_if_result(ctx, inst)?;
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Refuses a `convert.iconv.*` name php has no filter for, with php's own wording.
///
/// php answers `false` and warns `Unable to create or locate filter "<name>"` — the "create or
/// locate" verb rather than the plain "locate" one, because the `convert.iconv.` prefix DID
/// select a factory and the factory is what refused.
fn lower_refused_iconv_filter_attach(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    emit_iconv_create_warning(ctx, inst, prepend)?;
    emit_boxed_bool(ctx, false);
    store_if_result(ctx, inst)
}

/// Emits php's `Unable to create or locate filter "<name>"` for the call's filter-name operand.
fn emit_iconv_create_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    prepend: bool,
) -> Result<()> {
    let filter = expect_operand(inst, 1)?;
    load_string_to_result(ctx, filter, "stream_filter_append filter")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // the filter-name pointer
            ctx.emitter.instruction("mov x1, x2");                              // and its byte length
            ctx.emitter
                .instruction(&format!("mov x2, #{}", i64::from(prepend)));      // which function names itself
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // the filter-name pointer
            ctx.emitter.instruction("mov rsi, rdx");                            // and its byte length
            ctx.emitter
                .instruction(&format!("mov rdx, {}", i64::from(prepend)));      // which function names itself
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_filter_create_warning");
    Ok(())
}

/// Branches to `refused_label` unless `iconv_open()` can open this conversion.
///
/// php validates the charset pair when the filter is CREATED, so
/// `stream_filter_append($h, "convert.iconv.nope/alsonope")` answers `false` there and never
/// attaches. elephc attached regardless and only found out inside the transform, where the shape
/// silently leaves the bytes unconverted — so a typo'd charset looked like a working filter.
///
/// The probe descriptor is closed immediately; the transform opens its own.
fn emit_iconv_open_probe(
    ctx: &mut FunctionContext<'_>,
    from_sym: &str,
    to_sym: &str,
    refused_label: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // preserve the descriptor across the probe
            abi::emit_symbol_address(ctx.emitter, "x0", to_sym);
            abi::emit_symbol_address(ctx.emitter, "x1", from_sym);
            ctx.emitter.bl_c("iconv_open");
            ctx.emitter.instruction("cmn x0, #1");                              // is the descriptor (iconv_t)-1?
            ctx.emitter
                .instruction(&format!("b.eq __rt_icp_bad_{}", refused_label));
            ctx.emitter.bl_c("iconv_close");                                    // the transform opens its own
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // restore the stream descriptor
            let ok = format!("__rt_icp_ok_{}", refused_label);
            ctx.emitter.instruction(&format!("b {}", ok));
            ctx.emitter.label(&format!("__rt_icp_bad_{}", refused_label));
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // drop the preserved descriptor
            ctx.emitter.instruction(&format!("b {}", refused_label));
            ctx.emitter.label(&ok);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                             // preserve the descriptor across the probe
            ctx.emitter
                .instruction(&format!("lea rdi, [rip + {to_sym}]"));
            ctx.emitter
                .instruction(&format!("lea rsi, [rip + {from_sym}]"));
            ctx.emitter.instruction("call iconv_open");
            ctx.emitter.instruction("cmp rax, -1");                             // is the descriptor (iconv_t)-1?
            ctx.emitter
                .instruction(&format!("je __rt_icp_bad_{}", refused_label));
            ctx.emitter.instruction("mov rdi, rax");
            ctx.emitter.instruction("call iconv_close");                        // the transform opens its own
            abi::emit_pop_reg(ctx.emitter, "rax");                              // restore the stream descriptor
            let ok = format!("__rt_icp_ok_{}", refused_label);
            ctx.emitter.instruction(&format!("jmp {}", ok));
            ctx.emitter.label(&format!("__rt_icp_bad_{}", refused_label));
            abi::emit_pop_reg(ctx.emitter, "rax");                              // drop the preserved descriptor
            ctx.emitter.instruction(&format!("jmp {}", refused_label));
            ctx.emitter.label(&ok);
        }
    }
}

/// Picks the read or the write transcoder from `$mode` and attaches it to the descriptor already
/// held in the integer result register.
///
/// Shared by the literal attach and the dynamic-name one, which differ only in where the two
/// charset symbols come from: a deduplicated data string for a spec split at compile time, a
/// program-local buffer for one `__rt_iconv_spec_split` fills at run time. Both are addresses of
/// NUL-terminated bytes, which is all `iconv_open()` wants.
fn emit_iconv_transform_for_current_fd(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    from_sym: &str,
    to_sym: &str,
) -> Result<()> {
    let write_label = ctx.next_label("iconv_mode_write");
    let after_label = ctx.next_label("iconv_mode_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #-16]!");                     // preserve the descriptor across mode evaluation
            materialize_stream_filter_mode(ctx, inst, None)?;
            ctx.emitter.instruction("mov x9, x0");                              // hold the selected stream-filter mode
            ctx.emitter.instruction("ldr x0, [sp], #16");                       // restore the stream descriptor
            ctx.emitter.instruction("cmp x9, #2");                              // test for STREAM_FILTER_WRITE-only mode
            ctx.emitter.instruction(&format!("b.eq {}", write_label));          // install the streaming write transcoder
            emit_iconv_read_transform_for_current_fd(ctx, from_sym, to_sym);
            ctx.emitter.instruction(&format!("b {}", after_label));             // skip the write-filter attach path
            ctx.emitter.label(&write_label);
            emit_iconv_write_transform_for_current_fd(ctx, from_sym, to_sym);
            ctx.emitter.label(&after_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            materialize_stream_filter_mode(ctx, inst, None)?;
            ctx.emitter.instruction("mov r9, rax");                             // hold the selected stream-filter mode
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("cmp r9, 2");                               // test for STREAM_FILTER_WRITE-only mode
            ctx.emitter.instruction(&format!("je {}", write_label));            // install the streaming write transcoder
            emit_iconv_read_transform_for_current_fd(ctx, from_sym, to_sym);
            ctx.emitter.instruction(&format!("jmp {}", after_label));           // skip the write-filter attach path
            ctx.emitter.label(&write_label);
            emit_iconv_write_transform_for_current_fd(ctx, from_sym, to_sym);
            ctx.emitter.label(&after_label);
        }
    }
    Ok(())
}

/// Emits the attach-time READ transform for the current iconv stream descriptor.
pub(super) fn emit_iconv_read_transform_for_current_fd(
    ctx: &mut FunctionContext<'_>,
    from_sym: &str,
    to_sym: &str,
) {
    let labels = vec![
        ctx.next_label("iconv_slurp"),
        ctx.next_label("iconv_slurped"),
        ctx.next_label("iconv_sized"),
        ctx.next_label("iconv_skip"),
        ctx.next_label("iconv_write"),
        ctx.next_label("iconv_written"),
    ];
    let mut labels = labels.into_iter();
    match ctx.emitter.target.arch {
        Arch::AArch64 => crate::codegen::stream_filters::iconv::emit_read_arm64(
            ctx.emitter,
            from_sym,
            to_sym,
            |_| labels.next().expect("iconv read transform label"),
        ),
        Arch::X86_64 => crate::codegen::stream_filters::iconv::emit_read_x86_64(
            ctx.emitter,
            from_sym,
            to_sym,
            |_| labels.next().expect("iconv read transform label"),
        ),
    }
}

/// Emits the WRITE transform attachment for the current iconv stream descriptor.
pub(super) fn emit_iconv_write_transform_for_current_fd(
    ctx: &mut FunctionContext<'_>,
    from_sym: &str,
    to_sym: &str,
) {
    let labels = vec![
        ctx.next_label("iconv_w_fwrite"),
        ctx.next_label("iconv_w_close"),
        ctx.next_label("iconv_w_skip_helpers"),
        ctx.next_label("iconv_w_loop"),
        ctx.next_label("iconv_w_after_write"),
        ctx.next_label("iconv_w_done"),
        ctx.next_label("iconv_w_skip_store"),
    ];
    let mut labels = labels.into_iter();
    crate::codegen::stream_filters::iconv_write::emit_iconv_write_attach_with_labels(
        ctx.emitter,
        from_sym,
        to_sym,
        |_| labels.next().expect("iconv write transform label"),
    );
}

/// Boxes an opaque filter handle as a PHP resource value.
///
/// The payload is the registry handle, so `stream_filter_remove()` can resolve
/// the node again and `is_resource()` observes the registry lifetime.
pub(super) fn emit_boxed_filter_handle(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // resource payload = the filter handle
            // The high word is the registry-ownership marker, not padding: only the
            // marked values make is_resource()/get_resource_type() consult the
            // registry. Leaving it zero routed them down the legacy branch, which
            // answers "stream" and "open" unconditionally.
            ctx.emitter.instruction("mov x2, #1");                              // registry-owned resource marker
            ctx.emitter.instruction("mov x0, #9");                              // runtime tag 9 = resource
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // resource payload = the filter handle
            // The high word is the registry-ownership marker, not padding: only the
            // marked values make is_resource()/get_resource_type() consult the
            // registry. Leaving it zero routed them down the legacy branch, which
            // answers "stream" and "open" unconditionally.
            ctx.emitter.instruction("mov esi, 1");                              // registry-owned resource marker
            ctx.emitter.instruction("mov eax, 9");                              // runtime tag 9 = resource
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Throws php's `stream_filter_remove()` refusal unless the descriptor in the result register owns
/// a legacy per-descriptor filter.
///
/// The two direction tables hold one byte per descriptor per slot, zero when nothing is attached.
/// Reading all four is what separates a `zlib.deflate` handle — which the legacy teardown below
/// still owns — from an ordinary stream the caller passed by mistake.
fn emit_legacy_filter_presence_guard(ctx: &mut FunctionContext<'_>) {
    let present = ctx.next_label("sfr_legacy_present");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction("add x10, x0, #256");                       // the second slot for this descriptor
            ctx.emitter.instruction("ldrb w11, [x9, x0]");                      // read-direction slot 0
            ctx.emitter.instruction(&format!("cbnz w11, {present}"));
            ctx.emitter.instruction("ldrb w11, [x9, x10]");                     // read-direction slot 1
            ctx.emitter.instruction(&format!("cbnz w11, {present}"));
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction("ldrb w11, [x9, x0]");                      // write-direction slot 0
            ctx.emitter.instruction(&format!("cbnz w11, {present}"));
            ctx.emitter.instruction("ldrb w11, [x9, x10]");                     // write-direction slot 1
            ctx.emitter.instruction(&format!("cbnz w11, {present}"));
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters");
            ctx.emitter.instruction("lea r10, [rax + 256]");                    // the second slot for this descriptor
            ctx.emitter.instruction("movzx r11d, BYTE PTR [r9 + rax]");         // read-direction slot 0
            ctx.emitter.instruction("test r11d, r11d");
            ctx.emitter.instruction(&format!("jnz {present}"));
            ctx.emitter.instruction("movzx r11d, BYTE PTR [r9 + r10]");         // read-direction slot 1
            ctx.emitter.instruction("test r11d, r11d");
            ctx.emitter.instruction(&format!("jnz {present}"));
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters");
            ctx.emitter.instruction("movzx r11d, BYTE PTR [r9 + rax]");         // write-direction slot 0
            ctx.emitter.instruction("test r11d, r11d");
            ctx.emitter.instruction(&format!("jnz {present}"));
            ctx.emitter.instruction("movzx r11d, BYTE PTR [r9 + r10]");         // write-direction slot 1
            ctx.emitter.instruction("test r11d, r11d");
            ctx.emitter.instruction(&format!("jnz {present}"));
        }
    }
    emit_closed_stream_type_error(ctx, "stream_filter_remove");
    ctx.emitter.label(&present);
}

/// Lowers `stream_filter_remove(resource)`.
pub(crate) fn lower_stream_filter_remove(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "stream_filter_remove", 1)?;
    let filter = expect_operand(inst, 0)?;
    // A chain node resolves as a filter resource. Anything else is still a legacy
    // per-descriptor filter, so the previous teardown stays reachable until the
    // remaining families move over.
    let legacy = ctx.next_label("sfr_legacy");
    let refused = ctx.next_label("sfr_refused");
    let not_inert = ctx.next_label("sfr_not_inert");
    let done = ctx.next_label("sfr_done");
    load_stream_handle_to_result(ctx, filter, "stream_filter_remove")?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x0, [sp, #0]");                        // preserve the candidate handle
            abi::emit_call_label(ctx.emitter, "__rt_filter_state");
            ctx.emitter.instruction(&format!("cbz x0, {}", legacy));            // not a chain filter: use the legacy teardown
            // An INERT node stands for a filter whose real work is done by an inline shape over
            // the descriptor, so unlinking the node alone would leave that shape RUNNING: the
            // stream kept compressing after `stream_filter_remove()` said it had stopped. Both
            // facts are read now, while the node is still live.
            ctx.emitter.instruction(&format!("ldr x9, [x0, #{FILTER_FLAGS_OFFSET}]"));
            ctx.emitter.instruction(&format!("and x9, x9, #{FILTER_FLAG_INERT}"));
            ctx.emitter.instruction("str x9, [sp, #8]");                        // is this node inert?
            ctx.emitter.instruction(&format!("ldr x9, [x0, #{FILTER_STREAM_HANDLE_OFFSET}]"));
            ctx.emitter.instruction("str x9, [sp, #16]");                       // the stream whose tables it owns
            // PHP flushes a filter before removing it, and a PSFS_ERR_FATAL flush
            // CANCELS the removal: the filter stays attached and the call reports false.
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_filter_node_closing_flush");
            ctx.emitter.instruction(&format!("cbz x0, {}", refused));           // PSFS_ERR_FATAL: keep the filter attached
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the filter handle
            ctx.emitter.instruction(&format!("mov x1, #{STREAM_READ_FILTER_HEAD_OFFSET}"));
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_unlink");     // detach from the read chain
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the filter handle
            ctx.emitter.instruction(&format!("mov x1, #{STREAM_WRITE_FILTER_HEAD_OFFSET}"));
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_unlink");     // detach from the write chain
            // AFTER both chains: each unlink reads the node's own links and owning stream,
            // so clearing them inside it left the second call nothing to repair.
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_isolate");
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the filter handle
            // PHP closes a removed filter, so `onClose()` fires here rather than only
            // when the stream goes: the node is off both chains and the teardown that
            // sweeps them will never see it again.
            abi::emit_call_label(ctx.emitter, "__rt_filter_node_close_obj");    // onClose(), exactly once
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closed");     // publish Closed so is_resource() reports false
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");         // drop the reference stream_filter_append() handed out
            // Stop the inline shape too, or the stream keeps filtering after the removal.
            ctx.emitter.instruction("ldr x9, [sp, #8]");
            ctx.emitter.instruction(&format!("cbz x9, {}", not_inert));
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // the owning stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // x0 = its descriptor
            // php flushes the encoder's TAIL when the filter is removed, not only when the stream
            // closes: removing a `zlib.deflate` and then writing plain text puts the two-byte
            // deflate sync marker BEFORE that text. elephc emitted it at `fclose()` instead, so the
            // same bytes came out in the wrong order. The three helpers are the ones `fclose()`
            // already uses and each skips a descriptor that has no such filter, so calling all
            // three costs three loads for a filter that is none of them.
            emit_zlib_flush_on_close_for_current_fd(ctx);
            emit_bz2_flush_on_close_for_current_fd(ctx);
            emit_iconv_flush_on_close_for_current_fd(ctx);
            emit_legacy_filter_table_clear(ctx);
            ctx.emitter.label(&not_inert);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov x0, #1");                              // stream_filter_remove() reports success
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&refused);
            abi::emit_release_temporary_stack(ctx.emitter, 32);                 // the node stays linked and live
            ctx.emitter.instruction("mov x0, #0");                              // a refused flush reports false
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&legacy);
            ctx.emitter.instruction("ldr x0, [sp, #0]");                        // reload the candidate for the legacy path
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov QWORD PTR [rsp + 0], rax");            // preserve the candidate handle
            ctx.emitter.instruction("mov rdi, rax");                            // pass it to the filter-state probe
            abi::emit_call_label(ctx.emitter, "__rt_filter_state");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", legacy));                 // not a chain filter: use the legacy teardown
            // See the AArch64 counterpart: an inert node's real filtering lives in the tables.
            ctx.emitter.instruction(&format!(
                "mov r9, QWORD PTR [rax + {FILTER_FLAGS_OFFSET}]"
            ));
            ctx.emitter.instruction(&format!("and r9, {FILTER_FLAG_INERT}"));
            ctx.emitter.instruction("mov QWORD PTR [rsp + 8], r9");             // is this node inert?
            ctx.emitter.instruction(&format!(
                "mov r9, QWORD PTR [rax + {FILTER_STREAM_HANDLE_OFFSET}]"
            ));
            ctx.emitter.instruction("mov QWORD PTR [rsp + 16], r9");            // the stream whose tables it owns
            // See the AArch64 counterpart: a PSFS_ERR_FATAL flush cancels the removal.
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_filter_node_closing_flush");
            ctx.emitter.instruction("test rax, rax");
            ctx.emitter.instruction(&format!("jz {}", refused));                // PSFS_ERR_FATAL: keep the filter attached
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the filter handle
            ctx.emitter.instruction(&format!("mov rsi, {STREAM_READ_FILTER_HEAD_OFFSET}"));
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_unlink");     // detach from the read chain
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the filter handle
            ctx.emitter.instruction(&format!("mov rsi, {STREAM_WRITE_FILTER_HEAD_OFFSET}"));
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_unlink");     // detach from the write chain
            // AFTER both chains: each unlink reads the node's own links and owning stream,
            // so clearing them inside it left the second call nothing to repair.
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_filter_isolate");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the filter handle
            // See the AArch64 counterpart: a removed node is off both chains, so the
            // chain teardown can no longer fire its `onClose()`.
            abi::emit_call_label(ctx.emitter, "__rt_filter_node_close_obj");    // onClose(), exactly once
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_resource_mark_closed");     // publish Closed so is_resource() reports false
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the filter handle
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");         // drop the reference stream_filter_append() handed out
            // Stop the inline shape too, or the stream keeps filtering after the removal.
            ctx.emitter.instruction("mov r9, QWORD PTR [rsp + 8]");
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jz {}", not_inert));
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // the owning stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_fd");                // rax = its descriptor
            // See the AArch64 counterpart: php flushes the encoder tail at REMOVAL.
            emit_zlib_flush_on_close_for_current_fd(ctx);
            emit_bz2_flush_on_close_for_current_fd(ctx);
            emit_iconv_flush_on_close_for_current_fd(ctx);
            emit_legacy_filter_table_clear(ctx);
            ctx.emitter.label(&not_inert);
            abi::emit_release_temporary_stack(ctx.emitter, 32);
            ctx.emitter.instruction("mov eax, 1");                              // stream_filter_remove() reports success
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&refused);
            abi::emit_release_temporary_stack(ctx.emitter, 32);                 // the node stays linked and live
            ctx.emitter.instruction("xor eax, eax");                            // a refused flush reports false
            abi::emit_jump(ctx.emitter, &done);
            ctx.emitter.label(&legacy);
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the candidate for the legacy path
            abi::emit_release_temporary_stack(ctx.emitter, 32);
        }
    }
    load_stream_fd_to_result(ctx, filter, "stream_filter_remove")?;
    // A descriptor carrying no legacy filter is not a filter resource at all, and php refuses it:
    // `stream_filter_remove($stream)` on an ordinary handle throws rather than reporting success.
    // elephc reached this path for ANY resource the chain lookup rejected — a plain stream, or a
    // chain filter already removed — cleared four empty table slots and answered `true`. The four
    // slots are the legacy per-descriptor filters, which still serve `zlib.*` and `bzip2.*`, so the
    // path stays reachable for the handles that do own one.
    emit_legacy_filter_presence_guard(ctx);
    if matches!(ctx.emitter.target.arch, Arch::X86_64) {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the descriptor to the user-filter teardown helper
    }
    abi::emit_call_label(ctx.emitter, "__rt_user_filter_release_fd");
    emit_legacy_filter_table_clear(ctx);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction("mov x0, #1"),                 // return true after removing the filter state
        Arch::X86_64 => ctx.emitter.instruction("mov eax, 1"),
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Clears both per-descriptor filter slots in both directions, for the descriptor in the result
/// register.
///
/// This is what actually STOPS a `zlib.*`, `bzip2.*` or `convert.iconv.*` filter: those five run as
/// an inline shape keyed on the descriptor, not as a chain node, so unlinking their (inert) node
/// only retires the resource. Removing one without this left the stream compressing after
/// `stream_filter_remove()` had reported success.
fn emit_legacy_filter_table_clear(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_read_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear the read-direction slot 0
            ctx.emitter.instruction("add x10, x0, #256");                       // fd+256 (slot 1)
            ctx.emitter.instruction("strb wzr, [x9, x10]");                     // clear the read-direction slot 1
            abi::emit_symbol_address(ctx.emitter, "x9", "_stream_write_filters");
            ctx.emitter.instruction("strb wzr, [x9, x0]");                      // clear the write-direction slot 0
            ctx.emitter.instruction("strb wzr, [x9, x10]");                     // clear the write-direction slot 1
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_read_filters"); // read-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear the read-direction slot 0
            ctx.emitter.instruction("lea r10, [rax + 256]");                    // fd+256 (slot 1)
            ctx.emitter.instruction("mov BYTE PTR [r9 + r10], 0");              // clear the read-direction slot 1
            abi::emit_symbol_address(ctx.emitter, "r9", "_stream_write_filters"); // write-filter table base
            ctx.emitter.instruction("mov BYTE PTR [r9 + rax], 0");              // clear the write-direction slot 0
            ctx.emitter.instruction("mov BYTE PTR [r9 + r10], 0");              // clear the write-direction slot 1
        }
    }
}

