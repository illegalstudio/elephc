//! Purpose:
//! Resolves symbolic PHP token names from the shared constant catalog.
//!
//! Called from:
//! - The tokenizer C ABI, Magician and the lexer adapter.
//!
//! Key details:
//! - Single-byte tokens and unknown integer identifiers have no symbolic name.
//! - T_DOUBLE_COLON is the public name of the historical PAAMAYIM alias.

use std::collections::HashMap;
use std::sync::OnceLock;
use elephc_builtin_contract::{ConstValue, PhpModule};

/// Returns PHP's canonical token name, or None for punctuation and unknown IDs.
pub fn token_name(id: i64) -> Option<&'static str> {
    static NAMES: OnceLock<HashMap<i64, &'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = HashMap::new();
        for c in elephc_builtin_contract::constants() {
            if c.module == PhpModule::Tokenizer && c.name.starts_with("T_") && c.name != "T_PAAMAYIM_NEKUDOTAYIM" {
                if let ConstValue::Int(value) = c.value { names.insert(value, c.name); }
            }
        }
        names
    }).get(&id).copied()
}
