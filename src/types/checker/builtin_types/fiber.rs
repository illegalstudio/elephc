//! Purpose:
//! Builds and patches checker metadata for PHP builtin fiber types.
//! Supplies synthetic declarations or contract validation for classes and interfaces that user code may reference.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Dummy AST members carry type contracts only; runtime behavior is implemented elsewhere.

use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Expr, ExprKind, Stmt, StmtKind, Visibility};
use crate::types::PhpType;

use super::super::Checker;

/// Returns a dummy method body that returns `null`.
///
/// Used for Fiber methods whose runtime behavior is implemented elsewhere
/// in the codegen runtime. The dummy body satisfies the parser's requirement
/// that methods have a body.
fn fiber_method_dummy_body_return_null() -> Vec<Stmt> {
    vec![Stmt::new(
        StmtKind::Return(Some(Expr::new(
            ExprKind::Null,
            crate::span::Span::dummy(),
        ))),
        crate::span::Span::dummy(),
    )]
}

/// Returns a dummy method body that returns `false`.
///
/// Used for Fiber predicate methods (e.g., `isStarted`, `isTerminated`) whose
/// runtime behavior is implemented elsewhere in the codegen runtime. The dummy
/// body satisfies the parser's requirement that methods have a body.
fn fiber_method_dummy_body_return_false() -> Vec<Stmt> {
    vec![Stmt::new(
        StmtKind::Return(Some(Expr::new(
            ExprKind::BoolLiteral(false),
            crate::span::Span::dummy(),
        ))),
        crate::span::Span::dummy(),
    )]
}

/// Returns synthetic `ClassMethod` declarations for all Fiber builtin methods.
///
/// These dummy declarations provide the checker with method names, signatures,
/// and visibility so user code that references `Fiber` can be type-checked.
/// Actual runtime behavior is handled by the codegen fiber entry wrapper.
/// The returned methods include `__construct`, `start`, `resume`, `throw`,
/// `getReturn`, the four `is*` predicates, `suspend`, and `getCurrent`.
pub(super) fn builtin_fiber_methods() -> Vec<ClassMethod> {
    let span = crate::span::Span::dummy();
    let null_default = || Some(Expr::new(ExprKind::Null, span));
    let is_state_predicate = |name: &str| ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: true,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        return_type: None,
        body: fiber_method_dummy_body_return_false(),
        span,
        attributes: Vec::new(),
    };

    vec![
        // __construct(callable $callback): void
        ClassMethod {
            name: "__construct".to_string(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_final: true,
            has_body: true,
            params: vec![("callback".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: Vec::new(),
            span,
            attributes: Vec::new(),
        },
        // start(): mixed — bodies are dummy because codegen intercepts the call.
        // The checker patches this to seven optional Mixed parameters below;
        // the generated Fiber entry wrapper adapts those cells to the callback
        // ABI and keeps `use(...)` captures in reserved Fiber slots.
        ClassMethod {
            name: "start".to_string(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_final: true,
            has_body: true,
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: fiber_method_dummy_body_return_null(),
            span,
            attributes: Vec::new(),
        },
        // resume(?$value = null): mixed
        ClassMethod {
            name: "resume".to_string(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_final: true,
            has_body: true,
            params: vec![("value".to_string(), None, null_default(), false)],
            variadic: None,
            return_type: None,
            body: fiber_method_dummy_body_return_null(),
            span,
            attributes: Vec::new(),
        },
        // throw(Throwable $exception): mixed
        ClassMethod {
            name: "throw".to_string(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_final: true,
            has_body: true,
            params: vec![("exception".to_string(), None, None, false)],
            variadic: None,
            return_type: None,
            body: fiber_method_dummy_body_return_null(),
            span,
            attributes: Vec::new(),
        },
        // getReturn(): mixed
        ClassMethod {
            name: "getReturn".to_string(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_final: true,
            has_body: true,
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: fiber_method_dummy_body_return_null(),
            span,
            attributes: Vec::new(),
        },
        // isStarted/isSuspended/isRunning/isTerminated(): bool
        is_state_predicate("isStarted"),
        is_state_predicate("isSuspended"),
        is_state_predicate("isRunning"),
        is_state_predicate("isTerminated"),
        // static suspend($value = null): mixed
        ClassMethod {
            name: "suspend".to_string(),
            visibility: Visibility::Public,
            is_static: true,
            is_abstract: false,
            is_final: true,
            has_body: true,
            params: vec![("value".to_string(), None, null_default(), false)],
            variadic: None,
            return_type: None,
            body: fiber_method_dummy_body_return_null(),
            span,
            attributes: Vec::new(),
        },
        // static getCurrent(): ?Fiber
        ClassMethod {
            name: "getCurrent".to_string(),
            visibility: Visibility::Public,
            is_static: true,
            is_abstract: false,
            is_final: true,
            has_body: true,
            params: Vec::new(),
            variadic: None,
            return_type: None,
            body: fiber_method_dummy_body_return_null(),
            span,
            attributes: Vec::new(),
        },
    ]
}

/// Patches Fiber method signatures in the checker after initial class registration.
///
/// This function refines the parametric types of Fiber methods that were
/// registered with placeholder types. Specifically:
///
/// - `__construct`: first parameter becomes `Callable`, return type becomes `void`
/// - `start`: expands to 7 `Mixed` parameters with `null` defaults (matching the
///   AArch64 integer register limit), return type becomes `mixed`
/// - `resume`: first parameter becomes `mixed`, return type becomes `mixed`
/// - `throw`: first parameter becomes `Throwable`, return type becomes `mixed`
/// - `getReturn`: return type becomes `mixed`
/// - `isStarted`, `isSuspended`, `isRunning`, `isTerminated`: return type becomes `bool`
/// - `suspend` (static): first parameter becomes `mixed`, return type becomes `mixed`
/// - `getCurrent` (static): return type becomes `mixed`
///
/// Values transferred into or out of fibers are typed `mixed` so the codegen
/// boxes scalars into `Mixed` cells at call sites; the runtime transfers only
/// 8‑byte cell pointers.
pub(crate) fn patch_builtin_fiber_signatures(checker: &mut Checker) {
    // Values transferred in/out of a fiber are typed `mixed` so the codegen
    // boxes scalars (int, string, …) into Mixed cells at the call site. The
    // runtime then just shuffles 8-byte cell pointers through transfer_value;
    // the type tag rides along inside the heap cell that the pointer addresses.
    let throwable_ty = PhpType::Object("Throwable".to_string());
    let Some(class_info) = checker.classes.get_mut("Fiber") else {
        return;
    };

    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("__construct")) {
        if let Some(param) = sig.params.get_mut(0) {
            param.1 = PhpType::Callable;
        }
        sig.return_type = PhpType::Void;
    }
    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("start")) {
        // Allow up to 7 Mixed arguments to be forwarded to the fiber's closure
        // — that exhausts the AArch64 integer arg registers available after
        // $this. Each slot has a `null` default so $f->start() with no args
        // still type-checks, while $f->start($a, $b) fills slots 0..2 and
        // leaves slots 2..7 at the null default. `new Fiber(...)` validation
        // checks the callback signature and capture slot budgets separately.
        let span = crate::span::Span::dummy();
        sig.params = (0..7)
            .map(|i| (format!("arg{}", i), PhpType::Mixed))
            .collect();
        sig.defaults = (0..7)
            .map(|_| Some(Expr::new(ExprKind::Null, span)))
            .collect();
        sig.ref_params = vec![false; 7];
        sig.declared_params = vec![false; 7];
        sig.return_type = PhpType::Mixed;
    }
    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("resume")) {
        if let Some(param) = sig.params.get_mut(0) {
            param.1 = PhpType::Mixed;
        }
        sig.return_type = PhpType::Mixed;
    }
    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("throw")) {
        if let Some(param) = sig.params.get_mut(0) {
            param.1 = throwable_ty.clone();
        }
        sig.return_type = PhpType::Mixed;
    }
    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getReturn")) {
        sig.return_type = PhpType::Mixed;
    }
    for predicate in ["isStarted", "isSuspended", "isRunning", "isTerminated"] {
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(predicate)) {
            sig.return_type = PhpType::Bool;
        }
    }
    if let Some(sig) = class_info.static_methods.get_mut(&php_symbol_key("suspend")) {
        if let Some(param) = sig.params.get_mut(0) {
            param.1 = PhpType::Mixed;
        }
        sig.return_type = PhpType::Mixed;
    }
    if let Some(sig) = class_info.static_methods.get_mut(&php_symbol_key("getCurrent")) {
        sig.return_type = PhpType::Mixed;
    }
}
