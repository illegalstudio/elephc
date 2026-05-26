//! Purpose:
//! Defines the storage layout for runtime callable descriptor pointers.
//! Centralizes descriptor materialization and entry loading for indirect calls.
//!
//! Called from:
//! - Closure, first-class callable, callback, Fiber, and SPL callback emitters.
//!
//! Key details:
//! - `PhpType::Callable` remains one pointer-wide, but the pointer now targets a
//!   static descriptor whose entry slot is loaded before invoking native code.

use crate::codegen::abi;
use crate::codegen::data_section::{DataSection, DataWord};
use crate::codegen::emit::Emitter;

pub(crate) const CALLABLE_DESC_KIND_CLOSURE: u64 = 1;
pub(crate) const CALLABLE_DESC_KIND_FIRST_CLASS: u64 = 2;
pub(crate) const CALLABLE_DESC_KIND_CALLBACK_ADAPTER: u64 = 3;

pub(crate) const CALLABLE_DESC_ENTRY_OFFSET: usize = 8;

/// Provides the Static descriptor helper used by the callable descriptor module.
pub(crate) fn static_descriptor(
    data: &mut DataSection,
    entry_label: &str,
    php_name: Option<&str>,
    kind: u64,
) -> String {
    let (name_label, name_len) = match php_name {
        Some(name) => {
            let (label, len) = data.add_string(name.as_bytes());
            (Some(label), len as u64)
        }
        None => (None, 0),
    };

    let name_word = name_label
        .map(DataWord::Symbol)
        .unwrap_or(DataWord::U64(0));
    data.add_words(vec![
        DataWord::U64(kind),
        DataWord::Symbol(entry_label.to_string()),
        name_word,
        DataWord::U64(name_len),
        DataWord::U64(0),
        DataWord::U64(0),
        DataWord::U64(0),
    ])
}

/// Emits assembly for load descriptor address.
pub(crate) fn emit_load_descriptor_address(
    emitter: &mut Emitter,
    data: &mut DataSection,
    dest_reg: &str,
    entry_label: &str,
    php_name: Option<&str>,
    kind: u64,
) {
    let descriptor_label = static_descriptor(data, entry_label, php_name, kind);
    abi::emit_symbol_address(emitter, dest_reg, &descriptor_label);
}

/// Emits assembly for load entry from descriptor.
pub(crate) fn emit_load_entry_from_descriptor(
    emitter: &mut Emitter,
    dest_reg: &str,
    descriptor_reg: &str,
) {
    abi::emit_load_from_address(emitter, dest_reg, descriptor_reg, CALLABLE_DESC_ENTRY_OFFSET);
}
