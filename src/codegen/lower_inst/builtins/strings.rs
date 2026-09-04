//! Purpose:
//! Facade for EIR lowering of PHP string, hashing, compression, formatting, and network builtins.
//! Focused modules own each builtin family while sharing target-aware coercion and ABI helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch groups.
//! - Sibling IO builtin lowerers for shared hash and printf argument materialization.
//!
//! Key details:
//! - Runtime helpers retain ownership of returned string storage.
//! - Every backend path handles both AArch64 and x86_64 through the shared ABI layer.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::super::predicates;
use super::{
    expect_operand, io,
    load_value_to_first_int_arg, store_if_result,
};

mod common;
mod compression;
mod hash;
mod network;
mod parse_url;
mod printf;
mod replace_wrap;
mod scalar;
mod search;
mod simple;
mod split;

#[allow(unused_imports)]
use common::*;
#[allow(unused_imports)]
use compression::*;
#[allow(unused_imports)]
use hash::*;
#[allow(unused_imports)]
use network::*;
#[allow(unused_imports)]
use printf::*;
#[allow(unused_imports)]
use replace_wrap::*;
#[allow(unused_imports)]
use scalar::*;
#[allow(unused_imports)]
use search::*;
#[allow(unused_imports)]
use simple::*;
#[allow(unused_imports)]
use split::*;

pub(crate) use compression::{
    lower_gzcompress, lower_gzdeflate, lower_gzinflate, lower_gzuncompress,
};
pub(crate) use hash::{
    lower_crc32, lower_hash, lower_hash_algos, lower_hash_copy, lower_hash_equals,
    lower_hash_final, lower_hash_hmac, lower_hash_init, lower_hash_update, lower_mb_strlen,
    lower_md5, lower_sha1,
};
pub(crate) use network::{lower_inet, lower_ip2long, lower_long2ip};
pub(crate) use parse_url::lower_parse_url;
pub(crate) use printf::{lower_printf, lower_sprintf, lower_vprintf, lower_vsprintf};
pub(crate) use replace_wrap::{lower_str_pad, lower_string_replace, lower_wordwrap};
pub(crate) use scalar::{lower_chr, lower_number_format, lower_ord};
pub(crate) use search::{
    lower_str_contains, lower_str_repeat, lower_string_position, lower_strstr, lower_substr,
    lower_substr_replace,
};
pub(crate) use simple::{
    lower_binary_string_runtime, lower_grapheme_strrev, lower_html_escape, lower_lcfirst,
    lower_trim_like, lower_ucfirst,
};
pub(crate) use split::{lower_explode, lower_implode, lower_str_split};

#[allow(unused_imports)]
pub(super) use common::{
    load_single_string_arg, load_string_arg_to_regs, load_value_as_string_to_regs,
    materialize_truthy_flag,
};
#[allow(unused_imports)]
pub(super) use printf::{
    load_optional_sprintf_eval_context, pack_sprintf_like_arg, sprintf_spec_cats_for_format,
    SprintfSpecCat,
};

/// Materializes the optional `explode()` `$limit` into the splitter's extra argument register.
///
/// The already-materialized separator/subject pairs are parked while `$limit` is evaluated,
/// because coercing a non-integer limit can call runtime helpers that clobber the very
/// argument registers those pairs occupy. An omitted `$limit` becomes `PHP_INT_MAX`, which is
/// exactly how php-src spells "no limit" and lets the runtime helper share one code path.
fn load_split_limit_arg(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    let limit_reg = match ctx.emitter.target.arch {
        Arch::AArch64 => "x5",
        Arch::X86_64 => "rcx",
    };
    if inst.operands.len() < 3 {
        abi::emit_load_int_immediate(ctx.emitter, limit_reg, i64::MAX);
        return Ok(());
    }
    let limit = expect_operand(inst, 2)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // park the separator while the limit is materialized
            ctx.emitter.instruction("stp x3, x4, [sp, #-16]!");                 // park the subject string while the limit is materialized
            load_as_int(ctx, limit, &format!("{} limit", name))?;
            ctx.emitter.instruction("mov x5, x0");                              // pass the element limit as the extra splitter argument
            ctx.emitter.instruction("ldp x3, x4, [sp], #16");                   // restore the subject string into its splitter argument registers
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore the separator into its splitter argument registers
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");                 // park the separator while the limit is materialized
            abi::emit_push_reg_pair(ctx.emitter, "rdi", "rsi");                 // park the subject string while the limit is materialized
            load_as_int(ctx, limit, &format!("{} limit", name))?;
            ctx.emitter.instruction("mov rcx, rax");                            // pass the element limit as the extra splitter argument
            abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");                  // restore the subject string into its splitter argument registers
            abi::emit_pop_reg_pair(ctx.emitter, "rax", "rdx");                  // restore the separator into its splitter argument registers
        }
    }
    Ok(())
}

/// php-src's verbatim `ValueError` wording for a `base_convert()` `$from_base` outside 2..36.
const BASE_CONVERT_FROM_BASE_MESSAGE: &str =
    "base_convert(): Argument #2 ($from_base) must be between 2 and 36 (inclusive)";

/// php-src's verbatim `ValueError` wording for a `base_convert()` `$to_base` outside 2..36.
const BASE_CONVERT_TO_BASE_MESSAGE: &str =
    "base_convert(): Argument #3 ($to_base) must be between 2 and 36 (inclusive)";

/// php-src's verbatim `ValueError` wording for `chunk_split()` with a non-positive `$length`.
const CHUNK_SPLIT_NON_POSITIVE_LENGTH_MESSAGE: &str =
    "chunk_split(): Argument #2 ($length) must be greater than 0";

/// php-src's verbatim `ValueError` wording for `count_chars()` with an unknown `$mode`.
const COUNT_CHARS_MODE_MESSAGE: &str =
    "count_chars(): Argument #2 ($mode) must be between 0 and 4 (inclusive)";

/// php-src's verbatim `ValueError` wording for `explode()` with an empty `$separator`.
const EXPLODE_EMPTY_SEPARATOR_MESSAGE: &str =
    "explode(): Argument #1 ($separator) must not be empty";

/// php-src's verbatim `ValueError` wording, minus the leading function name, for a
/// `strpos()`-family `$offset` that does not land inside the haystack.
///
/// php-src emits the same sentence for `strpos()` and `strrpos()`, differing only in the
/// function name it is prefixed with, so the shared suffix is stored once and the caller
/// supplies the PHP spelling of the builtin being lowered.
const STRING_POSITION_OFFSET_OUT_OF_RANGE_SUFFIX: &str =
    "(): Argument #3 ($offset) must be contained in argument #1 ($haystack)";

/// php-src's verbatim `ValueError` wording for `strncasecmp()` with a negative `$length`.
const STRNCASECMP_NEGATIVE_LENGTH_MESSAGE: &str =
    "strncasecmp(): Argument #3 ($length) must be greater than or equal to 0";

/// php-src's verbatim `ValueError` wording for `strncmp()` with a negative `$length`.
const STRNCMP_NEGATIVE_LENGTH_MESSAGE: &str =
    "strncmp(): Argument #3 ($length) must be greater than or equal to 0";

/// php-src's verbatim `ValueError` wording for `str_repeat()` with a negative `$times`.
const STR_REPEAT_NEGATIVE_TIMES_MESSAGE: &str =
    "str_repeat(): Argument #2 ($times) must be greater than or equal to 0";

/// php-src's verbatim `ValueError` wording for `str_split()` with a non-positive `$length`.
const STR_SPLIT_NON_POSITIVE_LENGTH_MESSAGE: &str =
    "str_split(): Argument #2 ($length) must be greater than 0";

/// php-src's verbatim `ValueError` wording for `str_word_count()` with an unknown `$format`.
const STR_WORD_COUNT_FORMAT_MESSAGE: &str =
    "str_word_count(): Argument #2 ($format) must be a valid format value";

/// php-src's verbatim `ValueError` wording for `substr_count()` with an empty `$needle`.
const SUBSTR_COUNT_EMPTY_NEEDLE_MESSAGE: &str =
    "substr_count(): Argument #2 ($needle) must not be empty";

/// php-src's verbatim `ValueError` wording for a `substr_count()` `$offset` outside the subject.
const SUBSTR_COUNT_OFFSET_OUT_OF_RANGE_MESSAGE: &str =
    "substr_count(): Argument #3 ($offset) must be contained in argument #1 ($haystack)";

/// The scan direction of a `strpos()`-family builtin, which decides how `$offset` bounds
/// the searched window.
///
/// PHP resolves the third argument differently for the two directions: `strpos()` always
/// turns it into the first byte it may match at, while `strrpos()` turns a negative value
/// into the last byte a match may *end* on. Both spellings share one lowering, so the
/// direction is carried explicitly rather than re-derived from the runtime symbol name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringPositionDirection {
    /// Left-to-right search (`strpos()`).
    Forward,
    /// Right-to-left search (`strrpos()`).
    Reverse,
}

/// php-src's verbatim `ValueError` wording for `wordwrap()` with an empty `$break`.
const WORDWRAP_EMPTY_BREAK_MESSAGE: &str =
    "wordwrap(): Argument #3 ($break) must not be empty";

/// php-src's verbatim `ValueError` wording for a zero-width cutting `wordwrap()`.
const WORDWRAP_ZERO_WIDTH_CUT_MESSAGE: &str =
    "wordwrap(): Argument #4 ($cut_long_words) cannot be true when argument #2 ($width) is 0";

/// Rejects the `str_pad()` argument values reference PHP refuses to pad with.
///
/// `__rt_str_pad` copies `length - strlen($string)` bytes out of the pad string, so an
/// empty `$pad_string` would make it read whatever happens to follow the zero-length
/// buffer — that is the uninitialized `"xUUU"` output this guard removes. php-src checks
/// in exactly this order: a `$length` that cannot grow the input returns the input
/// untouched *before* either value check, then `$pad_string` emptiness, then `$pad_type`.
/// `has_pad_type` suppresses the fourth-argument guard for calls that leave `$pad_type`
/// defaulted, where `STR_PAD_RIGHT` is materialized as a constant and can never fail.
fn emit_str_pad_argument_guards(ctx: &mut FunctionContext<'_>, has_pad_type: bool) {
    let ok_label = ctx.next_label("str_pad_args_ok");
    let empty_pad_label = ctx.next_label("str_pad_empty_pad_string");
    let bad_type_label = ctx.next_label("str_pad_bad_pad_type");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x5, x2");                              // compare the requested length against the input length
            ctx.emitter.instruction(&format!("b.le {}", ok_label));             // PHP returns the input unchanged before validating anything else
            ctx.emitter.instruction(&format!("cbz x4, {}", empty_pad_label));   // an empty pad string cannot supply the missing bytes
            if has_pad_type {
                ctx.emitter.instruction("cmp x7, #2");                          // STR_PAD_LEFT/RIGHT/BOTH occupy 0..2
                ctx.emitter.instruction(&format!("b.hi {}", bad_type_label));   // any other pad mode, including negatives, is rejected
            }
            ctx.emitter.instruction(&format!("b {}", ok_label));                // both padding arguments are usable, so run the helper
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rcx, rdx");                            // compare the requested length against the input length
            ctx.emitter.instruction(&format!("jle {}", ok_label));              // PHP returns the input unchanged before validating anything else
            ctx.emitter.instruction("test rsi, rsi");                           // is the pad string empty?
            ctx.emitter.instruction(&format!("jz {}", empty_pad_label));        // an empty pad string cannot supply the missing bytes
            if has_pad_type {
                ctx.emitter.instruction("cmp r8, 2");                           // STR_PAD_LEFT/RIGHT/BOTH occupy 0..2
                ctx.emitter.instruction(&format!("ja {}", bad_type_label));     // any other pad mode, including negatives, is rejected
            }
            ctx.emitter.instruction(&format!("jmp {}", ok_label));              // both padding arguments are usable, so run the helper
        }
    }
    ctx.emitter.label(&empty_pad_label);
    super::super::exceptions::emit_value_error(ctx, STR_PAD_EMPTY_PAD_STRING_MESSAGE);
    if has_pad_type {
        ctx.emitter.label(&bad_type_label);
        super::super::exceptions::emit_value_error(ctx, STR_PAD_INVALID_PAD_TYPE_MESSAGE);
    }
    ctx.emitter.label(&ok_label);
}



/// Lowers `base64_decode(string, strict?)` and boxes its `string|false` answer as Mixed.
///
/// `__rt_base64_decode` reports a strict-mode rejection out of band — the decoded string
/// pair plus a separate success flag — because PHP's `false` and a successfully decoded
/// empty string are two different values that share the same empty pointer/length pair.
/// Both arms are boxed here, so the caller always receives one `Mixed` cell.
pub(crate) fn lower_base64_decode(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "base64_decode expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    if inst.result.is_some() && inst.result_php_type.codegen_repr() != PhpType::Mixed {
        // `crate::builtins::string::base64_decode::check` types EVERY call `string|false`,
        // whose representation is `Mixed`, and both arms below leave a BOXED cell in the
        // integer result register. A `Str` result type here would make `store_if_result` copy
        // the string-pair registers instead, which no longer hold the answer.
        return Err(CodegenIrError::invalid_module(format!(
            "base64_decode result must be Mixed (string|false), got {:?}",
            inst.result_php_type
        )));
    }
    let false_label = ctx.next_label("base64_decode_false");
    let end_label = ctx.next_label("base64_decode_end");
    // `$strict` is materialized FIRST and parked on the temporary stack: the truthiness
    // helpers clobber the same caller-saved registers the subject materialization needs.
    materialize_truthy_flag(ctx, inst, 1, "base64_decode")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            load_string_arg_to_regs(ctx, inst, 0, "base64_decode", "x1", "x2")?;
            abi::emit_pop_reg(ctx.emitter, "x3");                                // reload the parked $strict flag into the decoder's flag argument
            abi::emit_call_label(ctx.emitter, "__rt_base64_decode");
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // a strict decode that hit a bad character returns PHP's false
        }
        Arch::X86_64 => {
            load_string_arg_to_regs(ctx, inst, 0, "base64_decode", "rax", "rdx")?;
            abi::emit_pop_reg(ctx.emitter, "rdi");                               // reload the parked $strict flag into the decoder's flag argument
            abi::emit_call_label(ctx.emitter, "__rt_base64_decode");
            ctx.emitter.instruction("test r8, r8");                             // did the decoder accept the encoded input?
            ctx.emitter.instruction(&format!("jz {}", false_label));            // a strict decode that hit a bad character returns PHP's false
        }
    }
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.emitter.instruction(&format!("b {}", end_label)),  // skip the false arm once the decoded string is boxed
        Arch::X86_64 => ctx.emitter.instruction(&format!("jmp {}", end_label)), // skip the false arm once the decoded string is boxed
    }
    ctx.emitter.label(&false_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    ctx.emitter.label(&end_label);
    store_if_result(ctx, inst)
}

/// Lowers `ucwords(string, separators?)` with an explicit separator byte set.
///
/// `__rt_ucwords` always scans a caller-supplied set, so an omitted `$separators` is
/// materialized here as the address of `_ucwords_default_seps`. That keeps PHP's default
/// (`" \t\r\n\f\v"`, including the `\r`, `\f`, and `\v` the old hard-coded scan missed) and an
/// explicitly written set on exactly the same runtime path.
pub(crate) fn lower_ucwords(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.operands.is_empty() || inst.operands.len() > 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "ucwords expected 1 or 2 args, got {}",
            inst.operands.len()
        )));
    }
    let (subject_ptr, subject_len, sep_ptr, sep_len) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2", "x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi", "rdx", "rcx"),
    };
    if inst.operands.len() == 2 {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                load_string_arg_to_regs(ctx, inst, 0, "ucwords", "x1", "x2")?;
                ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");             // preserve the subject while the separator set is materialized
                load_string_arg_to_regs(ctx, inst, 1, "ucwords", "x3", "x4")?;
                ctx.emitter.instruction("ldp x1, x2, [sp], #16");               // restore the subject into the primary runtime string argument
            }
            Arch::X86_64 => {
                load_string_arg_to_regs(ctx, inst, 0, "ucwords", "rax", "rdx")?;
                abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
                load_string_arg_to_regs(ctx, inst, 1, "ucwords", "rax", "rdx")?;
                ctx.emitter.instruction("mov rcx, rdx");                        // pass the separator length as the fourth SysV argument
                ctx.emitter.instruction("mov rdx, rax");                        // pass the separator pointer as the third SysV argument
                abi::emit_pop_reg_pair(ctx.emitter, "rdi", "rsi");              // restore the subject into the primary SysV string arguments
            }
        }
    } else {
        load_string_arg_to_regs(ctx, inst, 0, "ucwords", subject_ptr, subject_len)?;
        abi::emit_symbol_address(ctx.emitter, sep_ptr, "_ucwords_default_seps");
        abi::emit_load_int_immediate(ctx.emitter, sep_len, UCWORDS_DEFAULT_SEPARATOR_COUNT);
    }
    abi::emit_call_label(ctx.emitter, "__rt_ucwords");
    store_if_result(ctx, inst)
}


/// php-src's verbatim `ValueError` wording for `str_pad()` with an empty `$pad_string`.
const STR_PAD_EMPTY_PAD_STRING_MESSAGE: &str =
    "str_pad(): Argument #3 ($pad_string) must not be empty";

/// php-src's verbatim `ValueError` wording for a `str_pad()` `$pad_type` outside 0..2.
const STR_PAD_INVALID_PAD_TYPE_MESSAGE: &str =
    "str_pad(): Argument #4 ($pad_type) must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH";

/// The byte length of `_ucwords_default_seps`, PHP's `" \t\r\n\f\v"` separator set.
const UCWORDS_DEFAULT_SEPARATOR_COUNT: i64 = 6;
pub(crate) use scalar::load_as_int;
pub(crate) use replace_wrap::lower_base_convert;
pub(crate) use split::lower_base_to_number;
pub(crate) use replace_wrap::lower_chunk_split;
pub(crate) use replace_wrap::lower_count_chars;
pub(crate) use split::lower_dec_to_base;
pub(crate) use split::lower_length_limited_compare;
pub(crate) use replace_wrap::lower_str_word_count;
pub(crate) use replace_wrap::lower_strtr;
pub(crate) use search::lower_substr_count;

/// php-src's verbatim `ValueError` wording for a `substr_count()` `$length` outside the subject.
const SUBSTR_COUNT_LENGTH_OUT_OF_RANGE_MESSAGE: &str =
    "substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)";
