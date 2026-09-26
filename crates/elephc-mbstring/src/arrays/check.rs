//! Purpose:
//! Checks PHP array keys and values for encoding validity with observable recursion warnings.
//!
//! Called from:
//! - The shared array-capable mb_check_encoding adapter.
//!
//! Key details:
//! - An invalid key ends that array traversal; invalid values still allow later warnings.
//! - Ancestor cycles fail, while repeated references outside the active path are rechecked.
//! - An explicit work stack avoids consuming the Rust call stack for nested arrays.

use super::{ArrayGraph, Key, Value};
use crate::encoding::Encoding;

/// PHP's boolean result plus warnings emitted in traversal order.
#[derive(Debug, PartialEq, Eq)]
pub struct ArrayCheck { pub valid: bool, pub warnings: Vec<&'static str> }

/// One active array traversal and its accumulated validation result.
struct Frame { array: usize, position: usize, valid: bool }

/// Validates every visited string key/value and rejects circular or unsupported values.
pub fn check_encoding(input: &ArrayGraph, encoding: Encoding) -> ArrayCheck {
    let mut protected = vec![false; input.arrays().len()];
    protected[input.root()] = true;
    let mut stack = vec![Frame { array: input.root(), position: 0, valid: true }];
    let mut warnings = Vec::new();
    while let Some(frame) = stack.last_mut() {
        let entries = &input.arrays()[frame.array];
        let Some((key, value)) = entries.get(frame.position) else {
            let completed = stack.pop().unwrap();
            protected[completed.array] = false;
            if let Some(parent) = stack.last_mut() { parent.valid &= completed.valid; }
            else { return ArrayCheck { valid: completed.valid, warnings }; }
            continue;
        };
        frame.position += 1;
        if matches!(key, Key::String(bytes) if !encoding.decode(bytes).is_valid()) {
            frame.valid = false;
            frame.position = entries.len();
            continue;
        }
        match value {
            Value::String(bytes) => frame.valid &= encoding.decode(bytes).is_valid(),
            Value::Array(array) => {
                if protected[*array] {
                    frame.valid = false;
                    warnings.push("mb_check_encoding(): Cannot not handle circular references");
                } else {
                    protected[*array] = true;
                    stack.push(Frame { array: *array, position: 0, valid: true });
                }
            }
            Value::Unsupported => frame.valid = false,
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => {},
        }
    }
    unreachable!("the validated root always produces a completed frame")
}
