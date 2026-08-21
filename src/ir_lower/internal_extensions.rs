//! Purpose:
//! Resolves PHP internal-extension operations and emits their typed EIR call instruction.
//! Keeps AST lowering independent from native bridge symbols and target ABI details.
//!
//! Called from:
//! - `crate::ir_lower::expr` for functions, constructors, methods, factories, and reads.
//! - `crate::ir_lower::stmt` for native virtual-property writes.
//!
//! Key details:
//! - Method and property lookup follows canonical aliases and inherited class metadata.
//! - Operand zero is the receiver exactly when `FLAG_RECEIVER` is present.

use crate::ir::{Immediate, Op, ValueId};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::span::Span;
use crate::types::PhpType;

/// Marks an internal-extension call whose first operand is a PHP object receiver.
pub(crate) const FLAG_RECEIVER: u32 = 1 << 0;

/// Marks a call whose bridge handle must be adopted into a fresh PHP wrapper.
pub(crate) const FLAG_WRAPPER_RESULT: u32 = 1 << 1;

/// Marks a call whose flat native fields must become an ordinary PHP value object.
pub(crate) const FLAG_VALUE_OBJECT_RESULT: u32 = 1 << 2;

/// Marks a SimpleXML object-handler offset synthesized from PHP's empty `[]` syntax.
///
/// The bridge must serialize this independently from a literal `null` offset:
/// php-src appends for the former and performs a regular null-key access for the latter.
pub(crate) const FLAG_ARRAY_APPEND_OFFSET: u32 = 1 << 3;

/// Returns the stable opcode for one exact generated internal operation key.
pub(crate) fn operation_opcode(key: &str) -> Option<u32> {
    crate::internal_extensions::operation_registry()
        .operation(key)
        .map(|operation| operation.opcode)
}

/// Returns the stable opcode for one internal-extension function.
pub(crate) fn function_opcode(name: &str) -> Option<u32> {
    crate::internal_extensions::operation_registry()
        .function(name)
        .map(|operation| operation.opcode)
}

/// Returns the stable opcode for a method, walking inherited class implementations.
pub(crate) fn method_opcode(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    method: &str,
) -> Option<u32> {
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(class_name) = current {
        if crate::internal_extensions::is_native_wrapper_class(&class_name) {
            if let Some(operation) =
                crate::internal_extensions::operation_registry().method(&class_name, method)
            {
                return Some(operation.opcode);
            }
        }
        current = internal_extension_parent(ctx, &class_name);
    }
    None
}

/// Returns the stable opcode for a virtual-property read or write through inheritance.
pub(crate) fn property_opcode(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    write: bool,
) -> Option<u32> {
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(class_name) = current {
        if crate::internal_extensions::is_native_wrapper_class(&class_name) {
            if let Some(operation) = crate::internal_extensions::operation_registry()
                .property(&class_name, property, write)
            {
                return Some(operation.opcode);
            }
        }
        current = internal_extension_parent(ctx, &class_name);
    }
    None
}

/// Returns one shared native property opcode for a wrapper or wrapper union.
///
/// Heterogeneous wrapper unions must agree on the exact opcode. When runtime
/// classes expose the same property through different native handlers, the
/// ordinary property instruction retains the union so codegen can dispatch by
/// the concrete object class instead of selecting the first union member.
pub(crate) fn property_opcode_for_type(
    ctx: &LoweringContext<'_, '_>,
    php_type: &PhpType,
    property: &str,
    write: bool,
) -> Option<u32> {
    match php_type {
        PhpType::Object(class_name) => {
            property_opcode(ctx, class_name, property, write)
        }
        PhpType::Union(members) => {
            let mut opcode = None;
            let mut found_wrapper = false;
            for member in members {
                match member {
                    PhpType::Object(class_name)
                        if crate::internal_extensions::is_native_wrapper_class(
                            class_name,
                        ) =>
                    {
                        let candidate =
                            property_opcode(ctx, class_name, property, write)?;
                        if opcode.is_some_and(|current| current != candidate) {
                            return None;
                        }
                        opcode.get_or_insert(candidate);
                        found_wrapper = true;
                    }
                    PhpType::Void
                    | PhpType::False
                    | PhpType::Bool
                    | PhpType::Int
                    | PhpType::Float
                    | PhpType::Str => {}
                    _ => return None,
                }
            }
            if found_wrapper {
                opcode
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Returns one SimpleXML object-handler opcode for a base wrapper or user subclass.
pub(crate) fn simplexml_object_handler_opcode_for_type(
    ctx: &LoweringContext<'_, '_>,
    php_type: &PhpType,
    handler: &str,
) -> Option<u32> {
    simplexml_object_result_type(ctx, php_type)?;
    crate::internal_extensions::operation_registry()
        .object_handler("simplexml", handler)
        .map(|operation| operation.opcode)
}

/// Returns the exact SimpleXML wrapper class represented by a direct or fallible loader type.
pub(crate) fn simplexml_object_result_type(
    ctx: &LoweringContext<'_, '_>,
    php_type: &PhpType,
) -> Option<PhpType> {
    match php_type {
        PhpType::Object(class_name) if is_simplexml_element_class(ctx, class_name) => {
            Some(PhpType::Object(class_name.clone()))
        }
        PhpType::Union(members) => {
            let mut result = None;
            for member in members {
                match member {
                    PhpType::Void | PhpType::Never | PhpType::False => {}
                    PhpType::Object(class_name)
                        if is_simplexml_element_class(ctx, class_name) =>
                    {
                        let candidate = PhpType::Object(class_name.clone());
                        if result.as_ref().is_some_and(|current| current != &candidate) {
                            return None;
                        }
                        result.get_or_insert(candidate);
                    }
                    _ => return None,
                }
            }
            result
        }
        _ => None,
    }
}

/// Returns whether a class is `SimpleXMLElement` or inherits from it.
pub(crate) fn is_simplexml_element_class(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> bool {
    let mut current = Some(class_name.trim_start_matches('\\').to_string());
    while let Some(class_name) = current {
        if class_name.eq_ignore_ascii_case("SimpleXMLElement") {
            return true;
        }
        current = internal_extension_parent(ctx, &class_name);
    }
    false
}

/// Returns one internal class parent from checker metadata or the locked source snapshot.
fn internal_extension_parent(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
) -> Option<String> {
    ctx.classes
        .get(class_name)
        .and_then(|class_info| class_info.parent.clone())
        .or_else(|| {
            crate::internal_extensions::registry()
                .class(class_name)
                .and_then(|class| class.parent.clone())
        })
}

/// Emits one typed, conservatively effectful internal-extension result.
pub(crate) fn emit_call(
    ctx: &mut LoweringContext<'_, '_>,
    opcode: u32,
    flags: u32,
    operands: Vec<ValueId>,
    result_type: PhpType,
    span: Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::InternalExtensionCall,
        operands,
        Some(Immediate::InternalExtension { opcode, flags }),
        result_type,
        Op::InternalExtensionCall.default_effects(),
        Some(span),
    )
}

/// Emits one effectful internal-extension operation without an EIR result.
pub(crate) fn emit_void_call(
    ctx: &mut LoweringContext<'_, '_>,
    opcode: u32,
    flags: u32,
    operands: Vec<ValueId>,
    span: Span,
) {
    ctx.emit_void(
        Op::InternalExtensionCall,
        operands,
        Some(Immediate::InternalExtension { opcode, flags }),
        Op::InternalExtensionCall.default_effects(),
        Some(span),
    );
}
