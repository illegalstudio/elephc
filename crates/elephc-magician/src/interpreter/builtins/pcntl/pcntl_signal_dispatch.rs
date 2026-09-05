//! Purpose:
//! Declares the Magician binding for `pcntl_signal_dispatch`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Queued bridge records are dispatched only at PHP-safe points.

eval_builtin! { contract: "pcntl_signal_dispatch", area: Pcntl, direct: Pcntl, values: Pcntl }
