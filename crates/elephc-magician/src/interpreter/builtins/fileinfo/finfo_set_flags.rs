//! Purpose:
//! Declares the eval binding for finfo_set_flags.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled fileinfo prelude.

eval_builtin! { contract: "finfo_set_flags", area: Filesystem, direct: Fileinfo, values: Fileinfo }
