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

#[derive(Clone)]
enum FinalArgSource {
    SourceTemp(usize),
    PrefixElement {
        prefix_temp_idx: usize,
        element_idx: usize,
        default: Option<Expr>,
    },
    Default(Expr),
}

#[derive(Clone)]
struct VariadicArgSource {
    key: Option<String>,
    source: FinalArgSource,
}

#[derive(Clone)]
struct PrefixVariadicTail {
    prefix_temp_idx: usize,
    start_idx: usize,
}
