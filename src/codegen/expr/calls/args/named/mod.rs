//! Purpose:
//! Defines shared data structures for named-argument source tracking and final argument sources.
//! Connects source-order evaluation, temporary storage, prefix spreads, and variadic construction.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Named argument lowering must consume the shared semantic plan instead of rebuilding matching rules.

mod final_args;
mod prefix;
mod source_order;
mod temps;
mod variadic;

pub(super) use source_order::emit_source_order_named_call_args;
pub(crate) use temps::pushed_temp_bytes;

use crate::parser::ast::Expr;

/// Describes the origin of a final call argument.
///
/// Variants cover: a previously-evaluated source temp, a prefix-array element
/// (with optional named-key preference and default fallback), or a plain default expression.
#[derive(Clone)]
enum FinalArgSource {
    SourceTemp(usize),
    PrefixElement {
        prefix_temp_idx: usize,
        element_idx: usize,
        prefer_named_key: bool,
        default: Option<Expr>,
    },
    Default(Expr),
}

/// Tracks one individual variadic argument's key and source.
///
/// The key is `None` for positional variadic elements and `Some(String)` for
/// named variadic arguments. The source follows the same taxonomy as regular
/// slot arguments.
#[derive(Clone)]
struct VariadicArgSource {
    key: Option<String>,
    source: FinalArgSource,
}

/// Records the source prefix array and the starting index for a variadic tail.
///
/// When a spread prefix array supplies elements beyond the regular parameter
/// count, this struct captures which prefix temp to read from and the first
/// variadic index in that prefix. Used to lazily splice prefix tail elements
/// into the variadic array during final argument materialization.
#[derive(Clone)]
struct PrefixVariadicTail {
    prefix_temp_idx: usize,
    start_idx: usize,
}
