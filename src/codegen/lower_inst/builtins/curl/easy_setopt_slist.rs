//! Purpose:
//! Lowers `__elephc_curl_easy_setopt_slist($handle, $option, $items)` — apply a
//! `struct curl_slist *`-valued option from the prelude's NUL-framed item blob.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - The C ABI shape is IDENTICAL to `elephc_curl_easy_setopt_str`'s (`id`, `opt`, `ptr`,
//!   `len`), so this shares `easy_setopt.rs`'s marshalling helper verbatim rather than
//!   repeating the staging order that file documents at length. Only the runtime helper
//!   it calls differs.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

/// Lowers `__elephc_curl_easy_setopt_slist($handle, $option, $items)`.
pub(crate) fn lower_curl_easy_setopt_slist(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::easy_setopt::lower_curl_setopt_bytes(
        ctx,
        inst,
        "__elephc_curl_easy_setopt_slist",
        "__rt_curl_easy_setopt_slist",
    )
}
