//! Purpose:
//! Builds and patches checker metadata for PHP builtin declarations types.
//! Supplies synthetic declarations or contract validation for classes and interfaces that user code may reference.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Dummy AST members carry type contracts only; runtime behavior is implemented elsewhere.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::traits::FlattenedClass;

use super::exception::{
    builtin_exception_code_property, builtin_exception_constructor_method,
    builtin_exception_get_code_method, builtin_exception_get_file_method,
    builtin_exception_get_line_method, builtin_exception_get_message_method,
    builtin_exception_get_previous_method, builtin_exception_get_trace_as_string_method,
    builtin_exception_get_trace_method, builtin_exception_message_property,
    builtin_exception_previous_property, builtin_exception_to_string_method,
    builtin_throwable_methods,
};
use super::fiber::builtin_fiber_methods;

/// Metadata for a builtin PHP interface declaration.
///
/// `name` is the fully-qualified interface name. `extends` lists parent interfaces.
/// `properties`, `methods`, and `constants` carry the type contract exposed to user code;
/// the checker consults these to validate member access without emitting runtime behavior.
pub(crate) struct InterfaceDeclInfo {
    pub name: String,
    pub extends: Vec<String>,
    pub properties: Vec<crate::parser::ast::ClassProperty>,
    pub methods: Vec<crate::parser::ast::ClassMethod>,
    pub span: crate::span::Span,
    pub constants: Vec<crate::parser::ast::ClassConst>,
}

impl Clone for InterfaceDeclInfo {
    /// Deep-copies all fields: name, extends list, properties, methods, span, and constants.
    fn clone(&self) -> Self {
        InterfaceDeclInfo {
            name: self.name.clone(),
            extends: self.extends.clone(),
            properties: self.properties.clone(),
            methods: self.methods.clone(),
            span: self.span,
            constants: self.constants.clone(),
        }
    }
}

/// Registers the builtin throwable hierarchy and Fiber declarations in
/// `interface_map` and `class_map`.
///
/// Checks for name collisions with user-declared types before inserting; returns
/// `CompileError` if any builtin name is already present. Insertion order sets
/// the inheritance chain: Error/Exception extend Throwable; TypeError/ValueError/
/// ArithmeticError/AssertionError/UnhandledMatchError extend Error;
/// ArgumentCountError extends TypeError; DivisionByZeroError extends
/// ArithmeticError; RuntimeException/ReflectionException extend Exception;
/// JsonException extends RuntimeException; FiberError extends Error. Fiber is
/// final with no parent.
///
/// The nominal parents mirror reference PHP 8.5.6 exactly, verified with
/// `php -d xdebug.mode=off -r 'var_dump(class_parents("ArgumentCountError"));'`
/// (`["TypeError", "Error"]`), the same probe for `DivisionByZeroError`
/// (`["ArithmeticError", "Error"]`) and `AssertionError` (`["Error"]`).
/// Builtin classes reference PHP reserves for internal use, which `new` must refuse.
///
/// The engine raises these itself and gives them no user-callable constructor, so
/// `new FiberError("boom")` is `Error: The "FiberError" class is reserved for internal use and
/// cannot be manually instantiated` there while it produced a working object here.
///
/// Kept next to the declaration list on purpose: the same edit that introduces a builtin
/// throwable is the one that has to answer whether PHP lets user code construct it. Most do —
/// `throw new RuntimeException(...)` is ordinary — which is why this cannot be inferred from
/// "is a builtin throwable" and has to be stated.
pub(crate) const RESERVED_FOR_INTERNAL_USE: [&str; 1] = ["FiberError"];

pub(crate) fn inject_builtin_throwables(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    wanted: &std::collections::HashSet<String>,
) -> Result<(), CompileError> {
    for builtin_name in [
        "Throwable",
        "Error",
        "TypeError",
        "ArgumentCountError",
        "ValueError",
        "ArithmeticError",
        "DivisionByZeroError",
        "AssertionError",
        "UnhandledMatchError",
        "Exception",
        "RuntimeException",
        "ReflectionException",
        "JsonException",
        "Fiber",
        "FiberError",
    ] {
        let builtin_key = php_symbol_key(builtin_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == builtin_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == builtin_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in type: {}", builtin_name),
            ));
        }
    }

    interface_map.insert(
        "Throwable".to_string(),
        InterfaceDeclInfo {
            name: "Throwable".to_string(),
            extends: Vec::new(),
            properties: Vec::new(),
            methods: builtin_throwable_methods(),
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );
    class_map.insert(
        "Error".to_string(),
        FlattenedClass {
            name: "Error".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec!["Throwable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: vec![
                builtin_exception_message_property(),
                builtin_exception_code_property(),
                builtin_exception_previous_property(),
            ],
            methods: vec![
                builtin_exception_constructor_method(),
                builtin_exception_get_message_method(),
                builtin_exception_get_code_method(),
                builtin_exception_get_file_method(),
                builtin_exception_get_line_method(),
                builtin_exception_get_trace_method(),
                builtin_exception_get_trace_as_string_method(),
                builtin_exception_get_previous_method(),
                builtin_exception_to_string_method(),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    class_map.insert(
        "Exception".to_string(),
        FlattenedClass {
            name: "Exception".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: vec!["Throwable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: vec![
                builtin_exception_message_property(),
                builtin_exception_code_property(),
                builtin_exception_previous_property(),
            ],
            methods: vec![
                builtin_exception_constructor_method(),
                builtin_exception_get_message_method(),
                builtin_exception_get_code_method(),
                builtin_exception_get_file_method(),
                builtin_exception_get_line_method(),
                builtin_exception_get_trace_method(),
                builtin_exception_get_trace_as_string_method(),
                builtin_exception_get_previous_method(),
                builtin_exception_to_string_method(),
            ],
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    // RuntimeException, ReflectionException, and JsonException inherit the
    // Throwable API from Exception via the standard inheritance machinery; they
    // don't need to redeclare anything locally.
    class_map.insert(
        "RuntimeException".to_string(),
        FlattenedClass {
            name: "RuntimeException".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Exception".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    class_map.insert(
        "ReflectionException".to_string(),
        FlattenedClass {
            name: "ReflectionException".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Exception".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    // `JsonException extends Exception`, DIRECTLY — verified against reference PHP 8.5.6 with
    // `php -n -r 'var_dump(class_parents("JsonException"));'`, which answers `["Exception"]`.
    // elephc used to put it under RuntimeException, which made
    // `catch (RuntimeException $e)` swallow a JSON error that PHP lets escape.
    class_map.insert(
        "JsonException".to_string(),
        FlattenedClass {
            name: "JsonException".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Exception".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    class_map.insert(
        "TypeError".to_string(),
        FlattenedClass {
            name: "TypeError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Error".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    // ArgumentCountError is the ONLY builtin Error subclass that is not a direct
    // child of Error: reference PHP nests it under TypeError, so
    // `catch (TypeError $e)` must catch it. Declaring it lets `catch`, `throw`,
    // `new`, and `instanceof` resolve the name; it inherits the whole Throwable
    // API transitively from Error.
    class_map.insert(
        "ArgumentCountError".to_string(),
        FlattenedClass {
            name: "ArgumentCountError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("TypeError".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    class_map.insert(
        "ValueError".to_string(),
        FlattenedClass {
            name: "ValueError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Error".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    class_map.insert(
        "ArithmeticError".to_string(),
        FlattenedClass {
            name: "ArithmeticError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Error".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    // DivisionByZeroError is what reference PHP raises for `$a / 0`, `$a % 0`, and
    // `intdiv($a, 0)` — an ArithmeticError subclass, so the wider
    // `catch (ArithmeticError $e)` still matches. The `intdiv()` zero-divisor
    // lowering in `crate::codegen::lower_inst::builtins::math::binary` throws it.
    class_map.insert(
        "DivisionByZeroError".to_string(),
        FlattenedClass {
            name: "DivisionByZeroError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("ArithmeticError".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    // AssertionError is a PHP builtin Error subclass raised by a failing `assert()`
    // under `zend.assertions=1`. Declaring it lets explicit new, throw, catch, and
    // instanceof resolve the class; elephc's `assert()` lowering does not construct
    // it (see the ERROR-class docs for that divergence).
    class_map.insert(
        "AssertionError".to_string(),
        FlattenedClass {
            name: "AssertionError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Error".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
    // UnhandledMatchError is a PHP builtin Error subclass. Declaring it lets explicit new, throw,
    // catch, and instanceof expressions resolve the class and inherit the Throwable API. The
    // current implicit no-match path remains a fatal EIR terminator and does not construct it.
    class_map.insert(
        "UnhandledMatchError".to_string(),
        FlattenedClass {
            name: "UnhandledMatchError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Error".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    // Fiber: cooperative coroutine class. Methods are placeholders here — the
    // codegen intercepts every Fiber operation (`new Fiber(...)`, instance
    // methods, `Fiber::suspend`, `Fiber::getCurrent`) and emits direct calls
    // into the `__rt_fiber_*` runtime helpers. Bodies are nominal returns so
    // the type checker sees a well-formed declaration.
    class_map.insert(
        "Fiber".to_string(),
        FlattenedClass {
            name: "Fiber".to_string(),
            span: crate::span::Span::dummy(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: true,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: builtin_fiber_methods(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    // FiberError: PHP models fiber state errors under Error, not Exception.
    class_map.insert(
        "FiberError".to_string(),
        FlattenedClass {
            name: "FiberError".to_string(),
            span: crate::span::Span::dummy(),
            extends: Some("Error".to_string()),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );

    // Drop the four the program cannot reach, AFTER the redeclaration check above has run over
    // the whole list — so `class ArgumentCountError {}` in user code is still rejected exactly as
    // before, whether or not the gate wanted ours. Removing here rather than gating each literal
    // block keeps the fourteen declarations reading as one table; building four `FlattenedClass`
    // values and dropping them costs nothing measurable next to flattening them.
    //
    // `builtin_throwable_gate` carries the reasoning for why these four and no others: three have
    // no producer anywhere in elephc, and `ReflectionException` has one only inside the Reflection
    // surface that its own gate decides.
    // `RuntimeException` is in this list because nothing raises it outside the SPL surface:
    // `_spl_runtime_exception_class_id` is read only by `runtime/spl/doubly_linked_list.rs`. It
    // was unconditional only while `JsonException` was wrongly declared to extend it. When a
    // program does reach it, `inject_builtin_spl_exceptions` puts it back — that injection owns
    // the whole SPL hierarchy and runs after this one.
    // `Fiber` and `FiberError` join the list on the same terms: nothing raises a FiberError
    // without a Fiber, and a program that never names either cannot make one. When absent,
    // `_fiber_class_id` and `_fiber_error_class_id` are emitted as `u64::MAX`, which no object
    // header carries, so the runtime comparisons never match.
    for builtin_name in [
        "ArgumentCountError",
        "AssertionError",
        "UnhandledMatchError",
        "ReflectionException",
        "RuntimeException",
        "Fiber",
        "FiberError",
    ] {
        if !wanted.contains(builtin_name) {
            class_map.remove(builtin_name);
        }
    }

    Ok(())
}
