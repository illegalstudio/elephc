//! Purpose:
//! Lowers PHP builtin functions that compile to a handful of WebAssembly instructions with no
//! runtime helper and no allocation, plus the audit contract each of them is admitted under.
//! This module registers the builtin WAT helpers and re-exports one submodule per family.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_runtime_call` for emission.
//! - `crate::codegen_wasm::capability::runtime_function_shape_issue` for the static audit.
//! - `crate::codegen_wasm::plan` for `emit_builtin_runtime` and the interned answer strings.
//!
//! Key details:
//! - Everything here is an EXACT identity: the WebAssembly instruction and the PHP builtin agree
//!   on every input including NaN, both infinities and negative zero, so there is no diagnostic
//!   to emit and no profile to branch on. A builtin that needs a table, an allocation or a
//!   warning does not belong in this module.
//! - The audit and the emitter read the same operand contract, so a shape the emitter cannot
//!   lower is refused before planning rather than producing an invalid module.
//! - Families (`strings`, `arrays`, `classes`, ...) own their lowerers and predicates; the
//!   `rt_*` modules own WAT text. `emit_builtin_runtime` keeps the registration order.

use super::context::{FnCtx, Result};
use super::wat::WatModule;
use super::inst::{operand, store_result};
use super::WasmError;
use crate::ir::{
    Function, Immediate, Instruction, IrHeapKind, IrType, Module, Op, RuntimeCallTarget,
    RuntimeFnId, UnaryStringRuntime, ValueId,
};
use crate::types::PhpType;

mod array_join;
mod array_mutate;
mod array_search;
mod array_sort;
mod arrays;
mod bcmath;
mod classes;
mod clock;
mod direct;
mod encoding;
mod extensions;
mod files;
mod format;
mod introspection;
mod network;
mod numbers;
mod operands;
mod output;
mod preg;
mod rt_arrays;
mod rt_digest;
mod rt_encoding;
mod rt_strings;
mod strings;
mod zlib;
#[cfg(test)]
mod tests;

pub(in crate::codegen_wasm) use array_search::*;
use array_join::*;
use array_mutate::*;
use array_sort::*;
use arrays::*;
use bcmath::*;
pub(in crate::codegen_wasm) use classes::*;
use clock::*;
pub(in crate::codegen_wasm) use direct::*;
use encoding::*;
pub(in crate::codegen_wasm) use extensions::*;
pub(in crate::codegen_wasm) use files::*;
pub(in crate::codegen_wasm) use format::*;
use introspection::*;
use network::*;
use numbers::*;
pub(in crate::codegen_wasm) use operands::*;
use output::*;
use preg::*;
use rt_arrays::*;
use rt_digest::*;
use rt_encoding::*;
use rt_strings::*;
pub(in crate::codegen_wasm) use strings::*;
use zlib::*;

/// Registers the WAT helpers the builtins in this module call.
///
/// Emitted for every module: none of these touch WASI directly. `has_main` selects whether
/// `chr`/`ord` can reach the PHP 8.5 deprecation helpers, which do — a reactor gets the same
/// answer with no diagnostic rather than a reference it cannot resolve.
pub(in crate::codegen_wasm) fn emit_builtin_runtime(wm: &mut WatModule, has_main: bool) {
    let diagnoses = has_main
        && matches!(
            crate::codegen_support::compile_php_version(),
            crate::web_prelude::PhpVersion::Php85
        );
    let (chr, ord) = str_chr_ord(diagnoses);
    wm.add_raw_func(&chr);
    wm.add_raw_func(&ord);
    wm.add_raw_func(RT_STR_REGION_EQ);
    wm.add_raw_func(RT_STR_CONTAINS);
    wm.add_raw_func(RT_STR_MAP_CASE);
    wm.add_raw_func(RT_STR_REVERSE);
    wm.add_raw_func(RT_ROUND);
    wm.add_raw_func(RT_POW10);
    wm.add_raw_func(RT_ROUND_PLACES);
    wm.add_raw_func(RT_STR_ALLOC);
    wm.add_raw_func(RT_STR_BIN2HEX);
    wm.add_raw_func(RT_STR_ADDSLASHES);
    wm.add_raw_func(RT_STR_STRIPSLASHES);
    wm.add_raw_func(RT_STR_NL2BR);
    wm.add_raw_func(RT_HEX_DIGIT_VALUE);
    wm.add_raw_func(RT_STR_URL_ENCODE);
    wm.add_raw_func(RT_STR_URL_DECODE);
    wm.add_raw_func(RT_B64_CHAR);
    wm.add_raw_func(RT_B64_VALUE);
    wm.add_raw_func(RT_STR_BASE64_ENCODE);
    wm.add_raw_func(RT_STR_BASE64_DECODE);
    wm.add_raw_func(RT_BASE64_DECODE);
    super::urls::emit_url_runtime(wm);
    super::inet::emit_inet_runtime(wm);
    super::arrays_ext::emit_array_ext_runtime(wm, has_main);
    super::arrays_parity::emit_array_parity_runtime(wm, has_main);
    wm.add_raw_func(RT_STR_CASE_EDGE);
    wm.add_raw_func(RT_STR_UCWORDS);
    wm.add_raw_func(RT_STR_CMP);
    wm.add_raw_func(RT_TRIM_MATCHES);
    wm.add_raw_func(RT_STR_TRIM);
    wm.add_raw_func(RT_STR_SUBSTR);
    wm.add_raw_func(RT_STR_REPEAT);
    wm.add_raw_func(RT_STR_FIND);
    wm.add_raw_func(RT_STR_RFIND);
    wm.add_raw_func(RT_IMPLODE);
    if has_main {
        // Compares through `__rt_str_loose_eq`, which lives in the command-only numeric runtime
        // because its diagnostics write to stderr. A reactor module has no such scan to make.
        wm.add_raw_func(RT_ARRAY_FIND_STR);
        // `__rt_sort_string` orders by `__rt_str_smart_cmp`, which lives in that same
        // command-only numeric runtime.
        wm.add_raw_func(super::arrays::RT_SORT_STRING);
    }
    wm.add_raw_func(RT_IMPLODE_OWNED);
    if has_main {
        // Walks its source through the generic `foreach` helpers, which are command-only.
        wm.add_raw_func(RT_IMPLODE_BOXED);
    }
    wm.add_raw_func(RT_EXPLODE);
    wm.add_raw_func(RT_STR_SPLIT);
    wm.add_raw_func(RT_WORDWRAP_GENERAL);
    wm.add_raw_func(RT_WORDWRAP);
    wm.add_raw_func(RT_FMT_PAD);
    wm.add_raw_func(RT_FMT_INT);
    wm.add_raw_func(RT_FMT_UINT_RADIX);
    wm.add_raw_func(RT_FMT_STR);
    wm.add_raw_func(RT_FMT_FLOAT);
    wm.add_raw_func(RT_PAD_BYTE);
    wm.add_raw_func(RT_STR_PAD);
    wm.add_raw_func(RT_STR_REPLACE);
    wm.add_raw_func(RT_CRC32);
    wm.add_raw_func(RT_SHA1_HEX);
    wm.add_raw_func(RT_MD5_K);
    wm.add_raw_func(RT_MD5_S);
    wm.add_raw_func(RT_MD5_HEX);
    wm.add_raw_func(RT_UTF8_SEQ_LEN);
    wm.add_raw_func(RT_UTF8_BAD_SPAN);
    wm.add_raw_func(RT_HTML_PUT);
    wm.add_raw_func(RT_HTMLSPECIALCHARS);
}
