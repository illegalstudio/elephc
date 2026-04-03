mod expr;
mod objects;
mod ops;
pub(super) mod syntactic;

pub use syntactic::{infer_expr_type_syntactic, infer_return_type_syntactic};
