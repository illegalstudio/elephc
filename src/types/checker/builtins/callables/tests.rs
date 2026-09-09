//! Purpose:
//! Pins callback input types for concrete containers and boxed PHP array contracts.
//!
//! Called from:
//! - The shared callable-checker unit tests.
//!
//! Key details:
//! - Opaque array storage must not invent integer values or integer-only keys.

use super::{array_element_type, array_key_type, PhpType};

/// PHP array unions preserve dynamic callback values and keys just like an explicit Mixed operand.
#[test]
fn boxed_array_callback_inputs_remain_dynamic() {
    for source in [PhpType::php_array(), PhpType::Mixed] {
        assert_eq!(array_element_type(&source), PhpType::Mixed);
        assert_eq!(array_key_type(&source), PhpType::Mixed);
    }
}

/// Concrete string and object containers retain their precise callback value and key types.
#[test]
fn concrete_array_callback_inputs_keep_their_types() {
    let packed = PhpType::Array(Box::new(PhpType::Str));
    assert_eq!(array_element_type(&packed), PhpType::Str);
    assert_eq!(array_key_type(&packed), PhpType::Int);
    let value = PhpType::Object("CallbackValue".to_string());
    let named = PhpType::AssocArray { key: Box::new(PhpType::Str), value: Box::new(value.clone()) };
    assert_eq!(array_element_type(&named), value);
    assert_eq!(array_key_type(&named), PhpType::Str);
}
