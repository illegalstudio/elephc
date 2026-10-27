//! Purpose:
//! Declares the eval binding for mime_content_type.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled fileinfo prelude.

eval_builtin! { contract: "mime_content_type", area: Filesystem, direct: Fileinfo, values: Fileinfo }
