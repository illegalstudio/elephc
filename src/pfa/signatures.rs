//! Purpose:
//! Collects the callable signatures partial function application copies onto its closure.
//!
//! Called from:
//! - `crate::pfa::desugar`, before the rewriting walk.
//!
//! Key details:
//! - User declarations are read from the name-resolved AST; builtins come from the shared
//!   contract catalog, so parameter names, types, defaults, and reference modes agree with
//!   ordinary calls.
//! - Method lookup follows the declared parent chain. A receiver whose class cannot be
//!   inferred still gets a closure; it just carries untyped parameters.

use std::cell::RefCell;
use std::collections::HashMap;

use elephc_builtin_contract::{BuiltinContract, DefaultSpec, TypeSpec};

use crate::magic_constants::walker::Pass;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{
    ClassMethod, Expr, ExprKind, MagicConstant, StaticReceiver, Stmt, StmtKind, TypeExpr,
};
use crate::span::Span;

/// One named parameter of the underlying callable.
#[derive(Clone)]
pub(super) struct ParamInfo {
    /// PHP-visible name, without the leading `$`.
    pub name: String,
    /// Declared type, if it can be copied into the closure's lexical scope.
    pub type_expr: Option<TypeExpr>,
    /// Declared default, if it is a constant expression that can be copied.
    pub default: Option<Expr>,
    /// Whether ordinary calls bind caller storage.
    pub by_ref: bool,
}

/// The information a partial application needs from the underlying callable.
#[derive(Clone)]
pub(super) struct CallableSig {
    /// Fixed parameters in source order.
    pub params: Vec<ParamInfo>,
    /// Variadic tail, if any.
    pub variadic: Option<ParamInfo>,
    /// Declared return type, when copyable.
    pub return_type: Option<TypeExpr>,
    /// Whether ordinary calls return caller-addressable storage.
    pub by_ref_return: bool,
    /// Whether the callable is a static method or a free function.
    pub is_static: bool,
}

/// Per-class method tables used while resolving `Class::method(?)` and `$obj->method(?)`.
#[derive(Default, Clone)]
struct ClassIndex {
    methods: HashMap<String, CallableSig>,
    static_methods: HashMap<String, CallableSig>,
    parent: Option<String>,
}

/// Complete signature index for one program.
#[derive(Default)]
pub(super) struct Signatures {
    functions: HashMap<String, CallableSig>,
    classes: HashMap<String, ClassIndex>,
}

impl Signatures {
    /// Collects declarations from the whole program.
    pub(super) fn collect(program: Vec<Stmt>) -> Self {
        let signatures = RefCell::new(Self::default());
        let mut collector = Collector { signatures: &signatures };
        let _ = crate::magic_constants::walker::walk_program(program, &mut collector);
        signatures.into_inner()
    }

    /// Looks up a free function by its resolved name.
    pub(super) fn function(&self, name: &Name) -> Option<CallableSig> {
        let key = php_symbol_key(name.as_str());
        if let Some(sig) = self.functions.get(&key) {
            return Some(sig.clone());
        }
        elephc_builtin_contract::lookup(name.as_str()).map(signature_from_contract)
    }

    /// Looks up an instance or static method through the declared parent chain.
    pub(super) fn method(&self, class_name: &str, method: &str, is_static: bool) -> Option<CallableSig> {
        let mut key = php_symbol_key(class_name);
        let method_key = php_symbol_key(method);
        let mut visited = std::collections::HashSet::new();
        while visited.insert(key.clone()) {
            let class = self.classes.get(&key)?;
            let table = if is_static { &class.static_methods } else { &class.methods };
            if let Some(sig) = table.get(&method_key) {
                return Some(sig.clone());
            }
            // A static call may name an instance method only when invoked through an object;
            // keeping the tables separate matches PHP's resolution.
            match class.parent.as_deref() {
                Some(parent) => key = php_symbol_key(parent),
                None => return None,
            }
        }
        None
    }

    /// Returns the declared parent of `class_name`, if any.
    pub(super) fn parent(&self, class_name: &str) -> Option<String> {
        self.classes
            .get(&php_symbol_key(class_name))
            .and_then(|class| class.parent.clone())
    }
}

/// Walks the rebuilt AST and stores declarations in the shared index.
struct Collector<'a> {
    signatures: &'a RefCell<Signatures>,
}

impl Pass for Collector<'_> {
    /// Leaves magic constants unchanged; they are already substituted before this pass.
    fn transform_magic(&self, _span: Span, magic: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(magic)
    }

    /// Records one declaration after its children have been walked.
    fn transform_stmt(&self, stmt: Stmt) -> Stmt {
        let mut signatures = self.signatures.borrow_mut();
        match &stmt.kind {
            StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                variadic_by_ref,
                variadic_type,
                return_type,
                by_ref_return,
                ..
            } => {
                signatures.functions.insert(
                    php_symbol_key(name),
                    signature_from_parts(
                        params,
                        variadic,
                        *variadic_by_ref,
                        variadic_type.as_ref(),
                        return_type.as_ref(),
                        *by_ref_return,
                    ),
                );
            }
            StmtKind::ClassDecl {
                name,
                extends,
                methods,
                ..
            } => {
                signatures.classes.insert(
                    php_symbol_key(name),
                    class_index(extends.as_ref(), methods),
                );
            }
            StmtKind::InterfaceDecl {
                name,
                methods,
                ..
            }
            | StmtKind::TraitDecl {
                name,
                methods,
                ..
            }
            | StmtKind::EnumDecl {
                name,
                methods,
                ..
            } => {
                signatures.classes.insert(
                    php_symbol_key(name),
                    class_index(None, methods),
                );
            }
            _ => {}
        }
        stmt
    }
}

/// Builds the method tables for one class-like declaration.
fn class_index(extends: Option<&Name>, methods: &[ClassMethod]) -> ClassIndex {
    let mut index = ClassIndex {
        parent: extends.map(Name::as_str).map(str::to_string),
        ..ClassIndex::default()
    };
    for method in methods {
        let signature = signature_from_parts(
            &method.params,
            &method.variadic,
            method.variadic_by_ref,
            method.variadic_type.as_ref(),
            method.return_type.as_ref(),
            method.by_ref_return,
        );
        let key = php_symbol_key(&method.name);
        let mut signature = signature;
        signature.is_static = method.is_static;
        if method.is_static {
            index.static_methods.insert(key, signature);
        } else {
            index.methods.insert(key, signature);
        }
    }
    index
}

/// Converts one declaration's parameter list into a partial-application signature.
fn signature_from_parts(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
    variadic_by_ref: bool,
    variadic_type: Option<&TypeExpr>,
    return_type: Option<&TypeExpr>,
    by_ref_return: bool,
) -> CallableSig {
    CallableSig {
        params: params
            .iter()
            .map(|(name, ty, default, by_ref)| ParamInfo {
                name: name.clone(),
                type_expr: ty.as_ref().and_then(copyable_type),
                default: default.as_ref().filter(|expr| copyable_default(expr)).cloned(),
                by_ref: *by_ref,
            })
            .collect(),
        variadic: variadic.as_ref().map(|name| ParamInfo {
            name: name.clone(),
            type_expr: variadic_type.and_then(copyable_type),
            default: None,
            by_ref: variadic_by_ref,
        }),
        return_type: return_type.and_then(copyable_return_type),
        by_ref_return,
        is_static: false,
    }
}

/// Converts a builtin contract into the same shape.
fn signature_from_contract(contract: &BuiltinContract) -> CallableSig {
    CallableSig {
        params: contract
            .params
            .iter()
            .map(|param| ParamInfo {
                name: param.name.to_string(),
                type_expr: type_spec_to_type_expr(&param.ty).and_then(|ty| copyable_type(&ty)),
                default: param.default.and_then(default_spec_to_expr),
                by_ref: param.by_ref,
            })
            .collect(),
        variadic: contract.variadic.map(|name| ParamInfo {
            name: name.to_string(),
            type_expr: None,
            default: None,
            by_ref: false,
        }),
        return_type: type_spec_to_type_expr(&contract.returns)
            .and_then(|ty| copyable_return_type(&ty)),
        by_ref_return: contract.by_ref_return,
        is_static: false,
    }
}

/// Converts a builtin type specification into an AST type expression.
fn type_spec_to_type_expr(spec: &TypeSpec) -> Option<TypeExpr> {
    Some(match spec {
        TypeSpec::Int => TypeExpr::Int,
        TypeSpec::Float => TypeExpr::Float,
        TypeSpec::Str => TypeExpr::Str,
        TypeSpec::Bool => TypeExpr::Bool,
        TypeSpec::Object => TypeExpr::Named(Name::unqualified("object")),
        TypeSpec::False => TypeExpr::False,
        TypeSpec::Null | TypeSpec::Mixed | TypeSpec::Ptr | TypeSpec::Callable => return None,
        TypeSpec::Union(members) => TypeExpr::Union(
            members
                .iter()
                .filter_map(type_spec_to_type_expr)
                .collect(),
        ),
        TypeSpec::Array => TypeExpr::Array(Box::new(TypeExpr::Named(Name::unqualified("mixed")))),
        TypeSpec::Nullable(inner) => {
            TypeExpr::Nullable(Box::new(type_spec_to_type_expr(inner)?))
        }
        TypeSpec::Void => TypeExpr::Void,
    })
}

/// Converts a builtin literal default into an AST expression.
fn default_spec_to_expr(spec: DefaultSpec) -> Option<Expr> {
    let span = Span::dummy();
    let kind = match spec {
        DefaultSpec::Null => ExprKind::Null,
        DefaultSpec::Int(value) => ExprKind::IntLiteral(value),
        DefaultSpec::Bool(value) => ExprKind::BoolLiteral(value),
        DefaultSpec::Float(value) => ExprKind::FloatLiteral(value),
        DefaultSpec::Str(value) => ExprKind::StringLiteral(value.to_string()),
        DefaultSpec::EmptyArray => ExprKind::ArrayLiteral(Vec::new()),
        DefaultSpec::Constant(name) => ExprKind::ConstRef(Name::unqualified(name)),
        DefaultSpec::ClassConstant { class, name } => ExprKind::ScopedConstantAccess {
            receiver: StaticReceiver::Named(Name::from_parts(
                crate::names::NameKind::FullyQualified,
                class
                    .split('\\')
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect(),
            )),
            name: name.to_string(),
        },
        DefaultSpec::IntMax | DefaultSpec::ErrorAll | DefaultSpec::Expr(_) => return None,
    };
    Some(Expr::new(kind, span))
}

/// Returns a type expression only when it is meaningful in the caller's lexical scope.
fn copyable_type(ty: &TypeExpr) -> Option<TypeExpr> {
    match ty {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Iterable => Some(ty.clone()),
        TypeExpr::Named(name) => {
            let lower = name.as_str().trim_start_matches('\\').to_ascii_lowercase();
            (!matches!(lower.as_str(), "self" | "parent" | "static") && !lower.contains('<'))
                .then(|| ty.clone())
        }
        TypeExpr::Array(element) => Some(TypeExpr::Array(Box::new(copyable_type(element)?))),
        TypeExpr::Nullable(inner) => Some(TypeExpr::Nullable(Box::new(copyable_type(inner)?))),
        TypeExpr::Union(members) => Some(TypeExpr::Union(
            members.iter().filter_map(copyable_type).collect(),
        )),
        TypeExpr::Void | TypeExpr::Never => None,
        TypeExpr::AssocArray { .. }
        | TypeExpr::Ptr(_)
        | TypeExpr::Buffer(_)
        | TypeExpr::GenericClass { .. }
        | TypeExpr::CallableSig { .. }
        | TypeExpr::Intersection(_) => None,
    }
}

/// Returns a return type when it is meaningful in the caller's lexical scope.
fn copyable_return_type(ty: &TypeExpr) -> Option<TypeExpr> {
    match ty {
        TypeExpr::Void | TypeExpr::Never => Some(ty.clone()),
        _ => copyable_type(ty),
    }
}

/// Returns whether a declared default can be copied into the generated closure verbatim.
fn copyable_default(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::ArrayLiteral(items) => items.iter().all(copyable_default),
        ExprKind::ArrayLiteralAssoc(entries) => entries
            .iter()
            .all(|(key, value)| copyable_default(key) && copyable_default(value)),
        ExprKind::ConstRef(name) => !matches!(
            name.as_str().trim_start_matches('\\').to_ascii_lowercase().as_str(),
            "self" | "parent" | "static"
        ),
        ExprKind::Negate(inner) | ExprKind::BitNot(inner) | ExprKind::Not(inner) => {
            copyable_default(inner)
        }
        ExprKind::BinaryOp { left, right, .. } => {
            copyable_default(left) && copyable_default(right)
        }
        ExprKind::ScopedConstantAccess { receiver, .. } => !matches!(
            receiver,
            StaticReceiver::Self_ | StaticReceiver::Parent | StaticReceiver::Static
        ),
        _ => false,
    }
}

/// Returns the class named by a static receiver, when it is statically known.
pub(super) fn receiver_class(
    receiver: &StaticReceiver,
    current_class: Option<&str>,
    signatures: &Signatures,
) -> Option<String> {
    match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => current_class.map(str::to_string),
        StaticReceiver::Parent => current_class.and_then(|class| signatures.parent(class)),
        StaticReceiver::Generic(class_type) => match class_type {
            TypeExpr::Named(name) => Some(name.as_str().to_string()),
            TypeExpr::GenericClass { name, .. } => Some(name.as_str().to_string()),
            _ => None,
        },
    }
}

/// Returns the declared class of a `new` expression, when it names one.
pub(super) fn new_expression_class(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::NewObject { class_name, .. } => Some(class_name.as_str().to_string()),
        ExprKind::NewScopedObject { receiver, .. } => match receiver {
            StaticReceiver::Named(name) => Some(name.as_str().to_string()),
            _ => None,
        },
        _ => None,
    }
}
