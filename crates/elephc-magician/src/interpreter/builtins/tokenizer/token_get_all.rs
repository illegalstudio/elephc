//! Purpose:
//! Registers PHP's token_get_all() in the eval builtin registry.
//!
//! Called from:
//! - Declarative builtin lookup and callable dispatch.
//!
//! Key details:
//! - Signature and module metadata come from the shared contract.

eval_builtin! { contract: "token_get_all", area: String, direct: Tokenizer, values: Tokenizer }
