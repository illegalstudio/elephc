//! Purpose:
//! Defines the WebAssembly local representation model for EIR values.
//! Maps each IrType to its corresponding WebAssembly local(s) and provides
//! utilities for declaring parameters and locals in WAT text format.
//!
//! Called from:
//! - `crate::codegen_wasm::function` when declaring a function's parameters,
//!   local slots, and SSA value locals.
//!
//! Key details:
//! - WebAssembly linear-memory addresses are i32 in wasm32, so pointers (Heap, Str ptr) are i32.
//! - PHP integers and string lengths follow PHP int semantics as i64.
//! - TaggedScalar mirrors the native "two words" representation (payload + small tag).
//! - Pointers are widened/wrapped at use sites; this module only declares the storage shape.

use crate::codegen_wasm::wat::{FuncBuilder, ValType};
use crate::ir::IrType;

/// How one EIR SSA value or local slot is realized as WebAssembly local(s).
/// Each `String` is a WAT local reference of the form "$name".
#[derive(Debug, Clone)]
pub enum WasmRepr {
    /// A single i64 local for PHP integer values.
    I64(String),
    /// A single f64 local for PHP floating-point values.
    F64(String),
    /// A single i32 local representing a linear-memory byte offset (pointer).
    Ptr(String),
    /// A string value represented as two locals: an i32 pointer and an i64 length.
    Str {
        /// The i32 pointer to the string data in linear memory.
        ptr: String,
        /// The i64 length of the string in bytes.
        len: String,
    },
    /// A tagged scalar represented as two locals: an i64 payload and an i32 tag.
    Tagged {
        /// The i64 payload value.
        payload: String,
        /// The i32 tag identifying the type/variant.
        tag: String,
    },
    /// Represents a void type with no storage.
    Void,
}

impl WasmRepr {
    /// Returns the WebAssembly value types backing the given IrType, in canonical order.
    ///
    /// # Arguments
    /// * `ir` - The EIR type to map to WebAssembly types.
    ///
    /// # Returns
    /// A vector of ValType representing the WebAssembly types needed to store a value
    /// of the given IrType. The order is canonical: for Str, [I32, I64] (ptr then len);
    /// for TaggedScalar, [I64, I32] (payload then tag).
    ///
    /// # Examples
    /// - IrType::I64 -> [ValType::I64]
    /// - IrType::Str -> [ValType::I32, ValType::I64]
    /// - IrType::Void -> []
    pub fn val_types(ir: IrType) -> Vec<ValType> {
        match ir {
            IrType::I64 => vec![ValType::I64],
            IrType::F64 => vec![ValType::F64],
            IrType::Heap(_) => vec![ValType::I32],
            IrType::Str => vec![ValType::I32, ValType::I64],
            IrType::TaggedScalar => vec![ValType::I64, ValType::I32],
            IrType::Void => vec![],
        }
    }

    /// Returns the WAT local references owned by this representation, in canonical order.
    ///
    /// # Returns
    /// A vector of "$name" strings. For Void, returns an empty vector.
    /// For Str, returns [ptr, len]. For Tagged, returns [payload, tag].
    ///
    /// # Examples
    /// - WasmRepr::I64("$x") -> ["$x"]
    /// - WasmRepr::Str { ptr: "$s_ptr", len: "$s_len" } -> ["$s_ptr", "$s_len"]
    /// - WasmRepr::Void -> []
    pub fn local_refs(&self) -> Vec<String> {
        match self {
            WasmRepr::I64(name) => vec![name.clone()],
            WasmRepr::F64(name) => vec![name.clone()],
            WasmRepr::Ptr(name) => vec![name.clone()],
            WasmRepr::Str { ptr, len } => vec![ptr.clone(), len.clone()],
            WasmRepr::Tagged { payload, tag } => vec![payload.clone(), tag.clone()],
            WasmRepr::Void => vec![],
        }
    }

    /// Returns true if this representation is Void (no storage).
    ///
    /// # Returns
    /// `true` if this is `WasmRepr::Void`, `false` otherwise.
    // Consumed by instruction lowering (skipping void instruction results) in a later phase.
    #[allow(dead_code)]
    pub fn is_void(&self) -> bool {
        matches!(self, WasmRepr::Void)
    }
}

/// Internal helper to declare a parameter or local for the given IrType.
///
/// # Arguments
/// * `fb` - The function builder to use for declaration.
/// * `base` - The base name for the local(s).
/// * `ir` - The EIR type to declare storage for.
/// * `declare_fn` - A closure that calls either `fb.param()` or `fb.local()`.
///
/// # Returns
/// The WasmRepr referencing the declared local(s).
fn declare_impl<F>(fb: &mut FuncBuilder, base: &str, ir: IrType, declare_fn: F) -> WasmRepr
where
    F: Fn(&mut FuncBuilder, &str, ValType) -> String,
{
    match ir {
        IrType::I64 => {
            let name = declare_fn(fb, base, ValType::I64);
            WasmRepr::I64(name)
        }
        IrType::F64 => {
            let name = declare_fn(fb, base, ValType::F64);
            WasmRepr::F64(name)
        }
        IrType::Heap(_) => {
            let name = declare_fn(fb, base, ValType::I32);
            WasmRepr::Ptr(name)
        }
        IrType::Str => {
            let ptr_name = declare_fn(fb, &format!("{}_ptr", base), ValType::I32);
            let len_name = declare_fn(fb, &format!("{}_len", base), ValType::I64);
            WasmRepr::Str {
                ptr: ptr_name,
                len: len_name,
            }
        }
        IrType::TaggedScalar => {
            let payload_name = declare_fn(fb, &format!("{}_pay", base), ValType::I64);
            let tag_name = declare_fn(fb, &format!("{}_tag", base), ValType::I32);
            WasmRepr::Tagged {
                payload: payload_name,
                tag: tag_name,
            }
        }
        IrType::Void => WasmRepr::Void,
    }
}

/// Declares WebAssembly parameter(s) for an EIR value of the given type.
///
/// # Arguments
/// * `fb` - The function builder to use for parameter declaration.
/// * `base` - The base name for the parameter(s). For single-value types, this is used
///   directly. For Str, generates `base_ptr` and `base_len`. For TaggedScalar, generates
///   `base_pay` and `base_tag`.
/// * `ir` - The EIR type to declare parameters for.
///
/// # Returns
/// A WasmRepr referencing the declared parameters. For `IrType::Void`, returns
/// `WasmRepr::Void` without declaring any parameters.
///
/// # Side Effects
/// Calls `fb.param()` for each required WebAssembly parameter, which modifies
/// the function builder's parameter list.
pub fn declare_param(fb: &mut FuncBuilder, base: &str, ir: IrType) -> WasmRepr {
    declare_impl(fb, base, ir, |fb, name, ty| fb.param(name, ty))
}

/// Declares WebAssembly local(s) for an EIR value of the given type.
///
/// # Arguments
/// * `fb` - The function builder to use for local declaration.
/// * `base` - The base name for the local(s). For single-value types, this is used
///   directly. For Str, generates `base_ptr` and `base_len`. For TaggedScalar, generates
///   `base_pay` and `base_tag`.
/// * `ir` - The EIR type to declare locals for.
///
/// # Returns
/// A WasmRepr referencing the declared locals. For `IrType::Void`, returns
/// `WasmRepr::Void` without declaring any locals.
///
/// # Side Effects
/// Calls `fb.local()` for each required WebAssembly local, which modifies
/// the function builder's local list.
pub fn declare_local(fb: &mut FuncBuilder, base: &str, ir: IrType) -> WasmRepr {
    declare_impl(fb, base, ir, |fb, name, ty| fb.local(name, ty))
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the EIR-value -> WebAssembly local representation model.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests exercise `val_types`/`local_refs`/`is_void`, which need no FuncBuilder.

    use super::*;
    use crate::ir::IrHeapKind;

    /// Verifies an integer maps to a single i64.
    #[test]
    fn val_types_i64_returns_i64() {
        let types = WasmRepr::val_types(IrType::I64);
        assert_eq!(types, vec![ValType::I64]);
    }

    /// Verifies a float maps to a single f64.
    #[test]
    fn val_types_f64_returns_f64() {
        let types = WasmRepr::val_types(IrType::F64);
        assert_eq!(types, vec![ValType::F64]);
    }

    /// Verifies a heap pointer maps to a single i32.
    #[test]
    fn val_types_heap_returns_i32() {
        let types = WasmRepr::val_types(IrType::Heap(IrHeapKind::Object));
        assert_eq!(types, vec![ValType::I32]);
    }

    /// Verifies a string maps to [i32 ptr, i64 len] in that order.
    #[test]
    fn val_types_str_returns_i32_i64() {
        let types = WasmRepr::val_types(IrType::Str);
        assert_eq!(types, vec![ValType::I32, ValType::I64]);
    }

    /// Verifies a tagged scalar maps to [i64 payload, i32 tag] in that order.
    #[test]
    fn val_types_tagged_returns_i64_i32() {
        let types = WasmRepr::val_types(IrType::TaggedScalar);
        assert_eq!(types, vec![ValType::I64, ValType::I32]);
    }

    /// Verifies void maps to no storage.
    #[test]
    fn val_types_void_returns_empty() {
        let types = WasmRepr::val_types(IrType::Void);
        assert!(types.is_empty());
    }

    /// Verifies a string repr lists ptr before len.
    #[test]
    fn local_refs_str_returns_ptr_then_len() {
        let repr = WasmRepr::Str {
            ptr: "$s_ptr".to_string(),
            len: "$s_len".to_string(),
        };
        let refs = repr.local_refs();
        assert_eq!(refs, vec!["$s_ptr", "$s_len"]);
    }

    /// Verifies a tagged repr lists payload before tag.
    #[test]
    fn local_refs_tagged_returns_payload_then_tag() {
        let repr = WasmRepr::Tagged {
            payload: "$t_pay".to_string(),
            tag: "$t_tag".to_string(),
        };
        let refs = repr.local_refs();
        assert_eq!(refs, vec!["$t_pay", "$t_tag"]);
    }

    /// Verifies a single-local repr lists just its one ref.
    #[test]
    fn local_refs_i64_returns_single() {
        let repr = WasmRepr::I64("$x".to_string());
        let refs = repr.local_refs();
        assert_eq!(refs, vec!["$x"]);
    }

    /// Verifies a float repr lists just its one ref.
    #[test]
    fn local_refs_f64_returns_single() {
        let repr = WasmRepr::F64("$y".to_string());
        let refs = repr.local_refs();
        assert_eq!(refs, vec!["$y"]);
    }

    /// Verifies a pointer repr lists just its one ref.
    #[test]
    fn local_refs_ptr_returns_single() {
        let repr = WasmRepr::Ptr("$p".to_string());
        let refs = repr.local_refs();
        assert_eq!(refs, vec!["$p"]);
    }

    /// Verifies a void repr owns no locals.
    #[test]
    fn local_refs_void_returns_empty() {
        let repr = WasmRepr::Void;
        let refs = repr.local_refs();
        assert!(refs.is_empty());
    }

    /// Verifies is_void is true only for Void.
    #[test]
    fn is_void_returns_true_for_void() {
        let repr = WasmRepr::Void;
        assert!(repr.is_void());
    }

    /// Verifies is_void is false for an integer repr.
    #[test]
    fn is_void_returns_false_for_i64() {
        let repr = WasmRepr::I64("$x".to_string());
        assert!(!repr.is_void());
    }

    /// Verifies is_void is false for a string repr.
    #[test]
    fn is_void_returns_false_for_str() {
        let repr = WasmRepr::Str {
            ptr: "$s_ptr".to_string(),
            len: "$s_len".to_string(),
        };
        assert!(!repr.is_void());
    }
}
