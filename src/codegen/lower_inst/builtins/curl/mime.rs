//! Purpose:
//! Lowers the five `curl_mime` builder builtins — `__elephc_curl_mime_new($handle)`,
//! `__elephc_curl_mime_add_part($handle)`, `__elephc_curl_mime_part_field($handle, $kind,
//! $value)`, `__elephc_curl_mime_post($handle)`, and `__elephc_curl_mime_abort($handle)`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_13`.
//!
//! Key details:
//! - FOUR OF THE FIVE ARE THE SAME `($handle)`-only SHAPE `lifecycle.rs` ALREADY LOWERS
//!   (`curl_reset`/`curl_upkeep`/`curl_copy_handle`), so they reuse
//!   `lifecycle::lower_handle_only` rather than a second copy of it.
//! - `__elephc_curl_mime_part_field` IS THE SAME `(handle, int, byte-string)` SHAPE
//!   `curl_setopt()`'s string setter already has (`__elephc_curl_easy_setopt_str`), so it
//!   reuses `easy_setopt::lower_curl_setopt_bytes` directly: `$kind` travels through the
//!   same "option" argument slot `lower_curl_setopt_bytes` already stages, and the value
//!   travels through the same ptr/len pair.

use crate::codegen::context::FunctionContext;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::easy_setopt::lower_curl_setopt_bytes;
use super::lifecycle::lower_handle_only;

/// Lowers `__elephc_curl_mime_new($handle)`.
pub(crate) fn lower_curl_mime_new(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_handle_only(
        ctx,
        inst,
        "__elephc_curl_mime_new",
        "curl_setopt",
        "__rt_curl_mime_new",
    )
}

/// Lowers `__elephc_curl_mime_add_part($handle)`.
pub(crate) fn lower_curl_mime_add_part(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_handle_only(
        ctx,
        inst,
        "__elephc_curl_mime_add_part",
        "curl_setopt",
        "__rt_curl_mime_add_part",
    )
}

/// Lowers `__elephc_curl_mime_part_field($handle, $kind, $value)`.
pub(crate) fn lower_curl_mime_part_field(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_curl_setopt_bytes(
        ctx,
        inst,
        "__elephc_curl_mime_part_field",
        "__rt_curl_mime_part_field",
    )
}

/// Lowers `__elephc_curl_mime_post($handle)`.
pub(crate) fn lower_curl_mime_post(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_handle_only(
        ctx,
        inst,
        "__elephc_curl_mime_post",
        "curl_setopt",
        "__rt_curl_mime_post",
    )
}

/// Lowers `__elephc_curl_mime_abort($handle)`.
pub(crate) fn lower_curl_mime_abort(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_handle_only(
        ctx,
        inst,
        "__elephc_curl_mime_abort",
        "curl_setopt",
        "__rt_curl_mime_abort",
    )
}
