//! Purpose:
//! Declares the Magician binding for `serialize`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution uses the shared boxed-cell runtime serializer; the host program
//!   contributes the class metadata for native objects such as `GMP`.

eval_builtin! {
    contract: "serialize",
    area: Core,
    direct: none,
    values: none,
}
