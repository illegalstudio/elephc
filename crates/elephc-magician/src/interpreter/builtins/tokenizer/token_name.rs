//! Purpose:
//! Registers PHP's token_name() in the eval builtin registry.
//!
//! Called from:
//! - Declarative builtin lookup and callable dispatch.
//!
//! Key details:
//! - Name resolution shares the same token catalog as the AOT bridge.

eval_builtin! { contract: "token_name", area: String, direct: Tokenizer, values: Tokenizer }
