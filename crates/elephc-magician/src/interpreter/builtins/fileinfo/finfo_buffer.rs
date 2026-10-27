//! Purpose:
//! Declares the eval binding for finfo_buffer.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled fileinfo prelude.

eval_builtin! { contract: "finfo_buffer", area: Filesystem, direct: Fileinfo, values: Fileinfo }
