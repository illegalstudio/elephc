//! Purpose:
//! Projects a static `stream_filter_append()`/`stream_filter_prepend()` `$params` array into
//! the trailing EIR operands the stream-filter codegen reads.
//!
//! Called from:
//! - `crate::ir_lower::expr::array_builtin_args::lower_builtin_call_args`.
//!
//! Key details:
//! - Layout appended after the PHP arguments: `level`, `blocks`, `work`, `write_seek_mode`.
//!   The first three use [`STREAM_FILTER_PARAM_UNSET`] when the caller omitted them; the seek
//!   mode uses 0 = preserve/absent, 1 = reset, 2 = strict, 3 = statically invalid.
//! - Only positional calls with a literal associative `$params` array are projected. Dynamic
//!   params keep their runtime value and the codegen falls back to the PHP defaults.
//! - `write_seek_mode` is PHP 8.6+; earlier profiles ignore the key like php-src does.

use super::*;

use crate::ir::STREAM_FILTER_PARAM_UNSET;

/// Appends the four static stream-filter parameter operands for a literal `$params` array.
pub(super) fn append_stream_filter_param_operands(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    mut operands: Vec<ValueId>,
) -> Vec<ValueId> {
    if canonical != "stream_filter_append" && canonical != "stream_filter_prepend" {
        return operands;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return operands;
    }
    let mut level = STREAM_FILTER_PARAM_UNSET;
    let mut blocks = STREAM_FILTER_PARAM_UNSET;
    let mut work = STREAM_FILTER_PARAM_UNSET;
    let mut seek_mode = 0i64;
    let mut params_span = None;
    if let Some(params) = args.get(3) {
        params_span = Some(params.span);
        for (key, value) in static_param_entries(params) {
            match key.as_str() {
                "level" => {
                    if let ExprKind::IntLiteral(number) = value.kind {
                        level = number;
                    }
                }
                "blocks" => {
                    if let ExprKind::IntLiteral(number) = value.kind {
                        blocks = number;
                    }
                }
                "work" => {
                    if let ExprKind::IntLiteral(number) = value.kind {
                        work = number;
                    }
                }
                "write_seek_mode" => {
                    seek_mode = static_write_seek_mode(value);
                }
                _ => {}
            }
        }
    }
    let span = params_span.unwrap_or_else(|| {
        args.first().map_or_else(Span::dummy, |arg| arg.span)
    });
    for value in [level, blocks, work, seek_mode] {
        operands.push(super::indexed_array_literals::emit_i64_at_span(ctx, value, span).value);
    }
    operands
}

/// Returns the string-keyed entries of a static associative `$params` literal.
fn static_param_entries(params: &Expr) -> Vec<(String, &Expr)> {
    let mut entries = Vec::new();
    match &params.kind {
        ExprKind::ArrayLiteralAssoc(pairs) => {
            for (key, value) in pairs {
                if let ExprKind::StringLiteral(key) = &key.kind {
                    entries.push((key.clone(), value));
                }
            }
        }
        ExprKind::ArrayLiteralMixed(items) => {
            for item in items {
                if let crate::parser::ast::ArrayEntry::Keyed(key, value) = item {
                    if let ExprKind::StringLiteral(key) = &key.kind {
                        entries.push((key.clone(), value));
                    }
                }
            }
        }
        _ => {}
    }
    entries
}

/// Maps a literal `write_seek_mode` value to its codegen mode.
///
/// php coerces the value to a string before matching, so every literal spelling other than the
/// three accepted names is invalid. A runtime value may still resolve to one of them, so it is
/// left unvalidated (0) instead of being guessed.
fn static_write_seek_mode(value: &Expr) -> i64 {
    match &value.kind {
        ExprKind::StringLiteral(mode) if mode == "preserve" => 0,
        ExprKind::StringLiteral(mode) if mode == "reset" => 1,
        ExprKind::StringLiteral(mode) if mode == "strict" => 2,
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => 3,
        _ => 0,
    }
}
