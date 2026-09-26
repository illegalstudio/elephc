//! Purpose:
//! Binds mb_send_mail to the shared mbstring runtime for eval calls.
//!
//! Called from:
//! - The Magician builtin registry.
//!
//! Key details:
//! - The shared contract owns parameter validation and the native bridge owns delivery.

eval_builtin! {
    contract: "mb_send_mail",
    area: String,
    direct: none,
    values: none,
}
