//! Purpose:
//! Joins Magician to the shared mb_strlen runtime contract.
//!
//! Called from:
//! - Magician builtin registry assembly and typed boxed-cell dispatch.
//!
//! Key details:
//! - The generated runtime calls the same mbstring bridge as AOT code.
//! - Magician owns no second codec implementation or request-state instance.

eval_builtin! {
    contract: "mb_strlen",
    area: String,
    direct: none,
    values: none,
}
