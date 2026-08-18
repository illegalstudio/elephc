//! Purpose:
//! Declares the Linux Magician binding for `pcntl_setcpuaffinity`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Input CPU identifiers are copied into a native vector before the bridge call.

eval_builtin! { contract: "pcntl_setcpuaffinity", area: Pcntl, direct: Pcntl, values: Pcntl }
