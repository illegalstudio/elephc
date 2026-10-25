//! Purpose:
//! Decides whether a parsed program references PHP's XML, XMLWriter, or SimpleXML
//! functions and classes, so the XML prelude is injected only for programs that use it.
//!
//! Called from:
//! - `crate::xml_prelude::inject_if_used`.
//!
//! Key details:
//! - Runs before the pipeline's name-resolution phase, so [`program_uses_xml`] first resolves
//!   a clone with the exact global xml function/class surface seeded as fallback symbols.
//!   This applies the authoritative PHP namespace/import rules without injecting the
//!   prelude: `namespace App; xml_parser_create()` falls back to the global function, while
//!   `\App\xml_parse()`, an imported user function, and a namespace-local declaration remain
//!   unrelated user symbols.
//! - TWO KINDS OF POSITION trigger injection: (a) a *function call* naming a catalog
//!   functions (including `xml_parse_into_struct`, a registry builtin whose lowering calls
//!   the prelude's helpers); (b) a *class-name* position naming a class from these modules
//!   (new, static receivers, `instanceof`, `catch`, `extends`/`implements`, type hints,
//!   trait uses, `use` imports). `XML_*` constants are NOT a trigger: they resolve from the
//!   shared constant catalog whether or not the prelude is injected.
//! - THE FUNCTION SET IS EXACT (the shared catalog's `Area::Xml` names), never a prefix, so
//!   a user `xml_helper()` does not pull in the bridge.
//! - A CLASS NAME THE RESOLVER LEAVES BARE still counts. The resolver rewrites the class
//!   names it visits to the seeded global form, but it copies a variadic parameter's
//!   element type verbatim and resolves `X::class` receivers without the seeded table, so
//!   `XMLParser ...$parsers` and `XMLWriter::class` stay the bare spelling; matching that
//!   spelling (one segment, any case) keeps those positions sound. Inside a namespace the
//!   bare spelling may over-inject for a same-named local class , the accepted trade-off.
//! - Soundness over precision: a missed reference would drop the prelude and turn a valid
//!   program into an "undefined function/class" error, so the `match`es are exhaustive (no
//!   wildcard arm). Adding an AST node forces this file to be updated. `eval()` is not a
//!   trigger: opaque eval source reaches the surface through `--with-xml`, exactly like
//!   the other bridge-backed preludes.

mod flow;

use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind,
    InstanceOfTarget, PackedField, StaticReceiver, Stmt, StmtKind, TraitAdaptation, TraitUse,
    TypeExpr,
};

/// The exact PHP functions of the xml surface: every `Area::Xml` contract in the shared
/// catalog. Whole-segment matches ensure user helpers with an `xml_` prefix do not silently
/// opt into the bridge.
fn xml_functions() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        elephc_builtin_contract::contracts()
            .iter()
            .filter(|contract| {
                contract.area == elephc_builtin_contract::Area::Xml && !contract.internal
            })
            .map(|contract| contract.name)
            .collect()
    })
}

/// The XMLParser, XMLWriter, SimpleXMLElement, and SimpleXMLIterator catalog classes.
fn xml_classes() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names: Vec<&'static str> = crate::types::builtin_classes::class_names_in_module(
            elephc_builtin_contract::PhpModule::Xml,
        )
        .to_vec();
        names.extend_from_slice(crate::types::builtin_classes::class_names_in_module(
            elephc_builtin_contract::PhpModule::Xmlwriter,
        ));
        names.extend_from_slice(crate::types::builtin_classes::class_names_in_module(
            elephc_builtin_contract::PhpModule::Simplexml,
        ));
        names
    })
}

/// Returns whether any statement resolves to the global xml surface, so the prelude must
/// be injected ahead of user code. Resolution errors are left for the pipeline's normal
/// name-resolution phase; an invalid program does not need speculative bridge injection.
pub(super) fn program_uses_xml(program: &[Stmt]) -> bool {
    let dom_available = crate::dom_html_prelude::program_uses_dom_html(program);
    let mut classes = xml_classes().to_vec();
    classes.push("DOMDocument");
    crate::name_resolver::resolve_with_additional_global_symbols(
        program.to_vec(), xml_functions(), &classes,
    )
    .is_ok_and(|resolved| {
        let detector = Detector { dom: super::dom_receiver::DomReceiver::new(&resolved, dom_available) };
        resolved.iter().any(|stmt| detector.stmt_refs_xml(stmt))
    })
}

/// Returns whether a resolved name is one of the xml global functions. Function names are
/// case-insensitive, but a name with namespace segments is unrelated to the global surface.
fn name_is_xml_function(name: &Name) -> bool {
    xml_functions()
        .iter()
        .any(|candidate| crate::name_resolver::is_additional_global_symbol(name, candidate))
}

/// Returns whether a resolved name is an XML module class: the seeded global the
/// resolver bound it to, or the bare single-segment spelling at a class-name position the
/// resolver leaves as written (a variadic parameter's element type, a `::class` or scoped
/// constant receiver). Class names are case-insensitive, but a name with namespace segments
/// is unrelated to the global surface.
fn name_is_xml_class(name: &Name) -> bool {
    xml_classes().iter().any(|candidate| {
        crate::name_resolver::is_additional_global_symbol(name, candidate)
            || matches!(name.parts.as_slice(), [bare] if bare.eq_ignore_ascii_case(candidate))
    })
}

/// Returns whether a static receiver names an xml class (`XMLWriter::toMemory()`).
/// `self`, `static`, and `parent` never resolve to a xml class at this position.
fn receiver_refs_xml(receiver: &StaticReceiver) -> bool {
    matches!(receiver, StaticReceiver::Named(name) if name_is_xml_class(name))
}

/// Returns whether a type expression names a xml class, recursing through
/// nullable/union/array/buffer wrappers and `ptr<Class>` targets.
fn type_refs_xml(type_expr: &TypeExpr) -> bool {
    match type_expr {
        TypeExpr::Int
        | TypeExpr::Float
        | TypeExpr::Bool
        | TypeExpr::False
        | TypeExpr::Str
        | TypeExpr::Void
        | TypeExpr::Never
        | TypeExpr::Iterable => false,
        TypeExpr::Ptr(target) => target.as_ref().is_some_and(name_is_xml_class),
        TypeExpr::Array(inner) | TypeExpr::Buffer(inner) | TypeExpr::Nullable(inner) => {
            type_refs_xml(inner)
        }
        TypeExpr::AssocArray { key, value } => type_refs_xml(key) || type_refs_xml(value),
        // A declared signature can name one of these classes, and a program
        // that mentions it only through a callback still needs the prelude.
        TypeExpr::CallableSig { params, ret } => {
            params.iter().any(type_refs_xml) || type_refs_xml(ret)
        }
        TypeExpr::Named(name) => name_is_xml_class(name),
        TypeExpr::GenericClass { name, args } => {
            name_is_xml_class(name) || args.iter().any(type_refs_xml)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_refs_xml)
        }
    }
}

/// Returns whether a `use Trait` clause names a xml class through its trait list or any
/// conflict-resolution adaptation.
fn trait_use_refs_xml(trait_use: &TraitUse) -> bool {
    trait_use.trait_names.iter().any(name_is_xml_class)
        || trait_use.adaptations.iter().any(|adaptation| match adaptation {
            TraitAdaptation::Alias { trait_name, .. } => {
                trait_name.as_ref().is_some_and(name_is_xml_class)
            }
            TraitAdaptation::InsteadOf {
                trait_name,
                instead_of,
                ..
            } => {
                trait_name.as_ref().is_some_and(name_is_xml_class)
                    || instead_of.iter().any(name_is_xml_class)
            }
        })
}

/// Returns whether a `packed class` field's type references a xml class. A xml class is
/// never a valid packed field type, but the field is walked for completeness.
fn packed_field_refs_xml(field: &PackedField) -> bool {
    type_refs_xml(&field.type_expr)
}

/// Receiver facts shared by the exhaustive XML reference walk.
struct Detector {
    dom: super::dom_receiver::DomReceiver,
}

impl Detector {
    /// Visits a function-like body with its lexical parameter and capture provenance.
    fn nested_body_refs_xml(
        &self,
        params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
        body: &[Stmt],
        capture: bool,
    ) -> bool {
        let detector = Self { dom: self.dom.nested(params, capture) };
        body.iter().any(|stmt| detector.stmt_refs_xml(stmt))
    }

    /// Returns whether an `instanceof` target references a xml class, recursing into the
    /// operand when the target is a runtime expression.
    fn instanceof_target_refs_xml(&self, target: &InstanceOfTarget) -> bool {
        match target {
            InstanceOfTarget::Name(name) => name_is_xml_class(name),
            InstanceOfTarget::Generic(class_type) => type_refs_xml(class_type),
            InstanceOfTarget::Expr(expr) => self.expr_refs_xml(expr),
        }
    }

    /// Returns whether a first-class-callable target references the xml surface: a free
    /// function (`xml_exec(...)`), a static-method receiver, or an instance-method object
    /// expression.
    fn callable_target_refs_xml(&self, target: &CallableTarget) -> bool {
        match target {
            CallableTarget::Function(name) => name_is_xml_function(name),
            CallableTarget::StaticMethod { receiver, .. } => receiver_refs_xml(receiver),
            CallableTarget::Method { object, method } => {
                (method.eq_ignore_ascii_case("loadXML") && self.dom.may_load_xml(object)) || self.expr_refs_xml(object)
            },
        }
    }

    /// Returns whether any parameter's type hint or default value references the xml
    /// surface. Shared by function, method, and closure parameter lists.
    fn params_ref_xml(&self, params: &[(String, Option<TypeExpr>, Option<Expr>, bool)]) -> bool {
        params.iter().any(|(_, type_expr, default, _)| {
            type_expr.as_ref().is_some_and(type_refs_xml)
                || default.as_ref().is_some_and(|value| self.expr_refs_xml(value))
        })
    }

    /// Returns whether a class property's type hint or default value references the xml
    /// surface.
    fn class_property_refs_xml(&self, property: &ClassProperty) -> bool {
        property.type_expr.as_ref().is_some_and(type_refs_xml)
            || property.default.as_ref().is_some_and(|value| self.expr_refs_xml(value))
    }

    /// Returns whether a method's parameters, variadic element type, return type, or body
    /// reference the xml surface.
    fn class_method_refs_xml(&self, method: &ClassMethod) -> bool {
        self.params_ref_xml(&method.params)
            || method.variadic_type.as_ref().is_some_and(type_refs_xml)
            || method.return_type.as_ref().is_some_and(type_refs_xml)
            || self.nested_body_refs_xml(&method.params, &method.body, false)
    }

    /// Returns whether a class constant's initializer references the xml surface.
    fn class_const_refs_xml(&self, constant: &ClassConst) -> bool {
        self.expr_refs_xml(&constant.value)
    }

    /// Returns whether an enum case's backing-value expression references the xml surface.
    fn enum_case_refs_xml(&self, case: &EnumCaseDecl) -> bool {
        case.value.as_ref().is_some_and(|value| self.expr_refs_xml(value))
    }

    /// Returns whether an expression references the xml surface at any function-call,
    /// class-name, or constant position, recursing into every child expression and statement.
    /// The `match` is exhaustive so a newly added `ExprKind` cannot silently bypass detection.
    fn expr_refs_xml(&self, expr: &Expr) -> bool {
        self.dom.observe_captures(expr);
        let mutations = self.dom.call_mutations(expr);
        let result = match &expr.kind {
            // Leaves and identifier-only forms carry no xml reference.
            ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::Variable(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::This
            | ExprKind::PreIncrement(_)
            | ExprKind::PostIncrement(_)
            | ExprKind::PreDecrement(_)
            | ExprKind::PostDecrement(_)
            | ExprKind::MagicConstant(_) => false,

            // `XML_*` constants resolve from the shared constant catalog with or without the
            // prelude, so a bare constant is not a trigger; the call or class-name position
            // that needs the prelude is caught elsewhere.
            ExprKind::ConstRef(_) => false,

            ExprKind::BinaryOp { left, op: crate::parser::ast::BinOp::And | crate::parser::ast::BinOp::Or, right } => {
                self.expr_refs_xml(left) || self.expression_alternatives_refs_xml(&[None, Some(right)])
            }
            ExprKind::BinaryOp { left, right, .. } => self.expr_refs_xml(left) || self.expr_refs_xml(right),
            ExprKind::InstanceOf { value, target } => {
                self.expr_refs_xml(value) || self.instanceof_target_refs_xml(target)
            }
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Throw(inner)
            | ExprKind::Clone(inner)
            | ExprKind::ErrorSuppress(inner)
            | ExprKind::Print(inner)
            | ExprKind::Spread(inner)
            | ExprKind::YieldFrom(inner) => self.expr_refs_xml(inner),
            ExprKind::NullCoalesce { value, default }
            | ExprKind::ShortTernary { value, default } => {
                self.expr_refs_xml(value) || self.expression_alternatives_refs_xml(&[None, Some(default)])
            }
            ExprKind::Pipe { value, callable } => self.expr_refs_xml(value) || self.expr_refs_xml(callable),
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                self.expr_refs_xml(target)
                    || self.expr_refs_xml(value)
                    || result_target.as_deref().is_some_and(|value| self.expr_refs_xml(value))
                    || prelude.iter().any(|value| self.stmt_refs_xml(value))
            }
            // A free-function call is the dominant xml position: `xml_parser_create()` and
            // `xmlwriter_open_memory()` mint the objects, so a program may parse or write
            // without ever naming a class.
            ExprKind::FunctionCall { name, args } => {
                name_is_xml_function(name) || self.callback_call_refs_xml(name, args) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::ClosureCall { args, .. } => args.iter().any(|value| self.expr_refs_xml(value)),
            ExprKind::ArrayLiteral(items) => items.iter().any(|value| self.expr_refs_xml(value)),
            ExprKind::ArrayLiteralAssoc(pairs) => pairs
                .iter()
                .any(|(key, value)| self.expr_refs_xml(key) || self.expr_refs_xml(value)),
            ExprKind::ArrayLiteralMixed(entries) => entries
                .iter()
                .flat_map(|entry| entry.exprs())
                .any(|value| self.expr_refs_xml(value)),
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                self.expr_refs_xml(subject)
                    || arms.iter().any(|(conditions, body)| {
                        conditions.iter().any(|value| self.expr_refs_xml(value)) || self.expr_refs_xml(body)
                    })
                    || default.as_deref().is_some_and(|value| self.expr_refs_xml(value))
            }
            ExprKind::ArrayAccess { array, index } => self.expr_refs_xml(array) || self.expr_refs_xml(index),
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => self.expr_refs_xml(condition) || self.expression_alternatives_refs_xml(&[Some(then_expr), Some(else_expr)]),
            ExprKind::Cast { expr, .. } | ExprKind::PtrCast { expr, .. } => self.expr_refs_xml(expr),
            ExprKind::Closure {
                params,
                variadic_type,
                return_type,
                body,
                ..
            } => {
                self.params_ref_xml(params)
                    || variadic_type.as_ref().is_some_and(type_refs_xml)
                    || return_type.as_ref().is_some_and(type_refs_xml)
                    || self.nested_body_refs_xml(params, body, true)
            }
            ExprKind::NamedArg { value, .. } => self.expr_refs_xml(value),
            ExprKind::ExprCall { callee, args } => {
                self.expr_refs_xml(callee) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::NewObject { class_name, args } => {
                name_is_xml_class(class_name) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::NewGeneric { class_type, args } => {
                type_refs_xml(class_type) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::NewDynamic { name_expr, args } => {
                self.expr_refs_xml(name_expr) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::NewDynamicObject {
                class_name,
                fallback_class,
                required_parent,
                args,
            } => {
                self.expr_refs_xml(class_name)
                    || name_is_xml_class(fallback_class)
                    || name_is_xml_class(required_parent)
                    || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::NullsafePropertyAccess { object, .. } => self.expr_refs_xml(object),
            ExprKind::DynamicPropertyAccess { object, property } => {
                self.expr_refs_xml(object) || self.expr_refs_xml(property)
            }
            ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
                self.expr_refs_xml(object) || self.expression_alternatives_refs_xml(&[None, Some(property)])
            }
            ExprKind::StaticPropertyAccess { receiver, .. } => receiver_refs_xml(receiver),
            ExprKind::MethodCall { object, method, args, .. } => {
                (method.eq_ignore_ascii_case("loadXML") && self.dom.may_load_xml(object)) || self.expr_refs_xml(object) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::NullsafeMethodCall { object, method, args, .. } => {
                (method.eq_ignore_ascii_case("loadXML") && self.dom.may_load_xml(object)) || self.expr_refs_xml(object) || self.optional_args_refs_xml(args)
            }
            ExprKind::NullsafeDynamicMethodCall {
                object,
                method,
                args,
            } => {
                if self.dynamic_method_refs_xml(object, method) || self.expr_refs_xml(object) { return true; }
                let called = self.fork();
                let result = called.expr_refs_xml(method) || args.iter().any(|value| called.expr_refs_xml(value));
                self.dom.join(&called.dom);
                result
            },
            ExprKind::StaticMethodCall { receiver, args, .. } => {
                receiver_refs_xml(receiver) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::FirstClassCallable(target) => self.callable_target_refs_xml(target),
            ExprKind::BufferNew { element_type, len } => {
                type_refs_xml(element_type) || self.expr_refs_xml(len)
            }
            ExprKind::ClassConstant { receiver }
            | ExprKind::ScopedConstantAccess { receiver, .. } => receiver_refs_xml(receiver),
            ExprKind::ObjectClassName { object } => self.expr_refs_xml(object),
            ExprKind::NewScopedObject { receiver, args } => {
                receiver_refs_xml(receiver) || args.iter().any(|value| self.expr_refs_xml(value))
            }
            ExprKind::Yield { key, value } => {
                key.as_deref().is_some_and(|value| self.expr_refs_xml(value))
                    || value.as_deref().is_some_and(|value| self.expr_refs_xml(value))
            }
            // Transient: the resolver expands this into the included file's statements before
            // xml detection runs, so it should never reach here. Recurse into the path
            // expression defensively to keep detection exhaustive and correct.
            ExprKind::IncludeValue { path, .. } => self.expr_refs_xml(path),
        };
        self.dom.apply_mutations(&mutations);
        self.dom.observe_expression(expr);
        result
    }

    /// Returns whether a statement references the xml surface at any function-call,
    /// class-name, or constant position, recursing into nested statements, expressions, and
    /// class members. The `match` is exhaustive so a newly added `StmtKind` cannot silently
    /// bypass detection.
    fn stmt_refs_xml(&self, stmt: &Stmt) -> bool {
        let result = match &stmt.kind {
            // Statements with no xml-name position and no child expr/stmt.
            StmtKind::RefAssign { .. }
            | StmtKind::IncludeOnceMark { .. }
            | StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::NamespaceDecl { .. }
            | StmtKind::FunctionVariantGroup { .. }
            | StmtKind::FunctionVariantMark { .. }
            | StmtKind::Global { .. }
            | StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => false,

            // An aliased import (`use XMLWriter as W;`) names the class only here; the later
            // `new W()` / `W $w` carries the alias, which the walk cannot otherwise connect
            // back , so skipping imports would be a false negative. Function imports
            // (`use function xml_parse as p;`) land in the same list and are caught by the
            // same check, which is why both name tests are applied.
            StmtKind::UseDecl { imports } => imports
                .iter()
                .any(|item| name_is_xml_class(&item.name) || name_is_xml_function(&item.name)),

            StmtKind::Echo(expr) | StmtKind::Throw(expr) | StmtKind::ExprStmt(expr) => {
                self.expr_refs_xml(expr)
            }
            StmtKind::Assign { value, .. } => self.expr_refs_xml(value),
            StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
                self.if_refs_xml(condition, then_body, elseif_clauses, else_body.as_deref())
            }
            StmtKind::IfDef { then_body, else_body, .. } => {
                self.alternatives_refs_xml([then_body.as_slice(), else_body.as_deref().unwrap_or(&[])])
            }
            StmtKind::While { condition, body } => self.loop_refs_xml(|path| {
                path.expr_refs_xml(condition) || path.body_refs_xml(body)
            }),
            StmtKind::DoWhile { body, condition } => {
                self.body_refs_xml(body) || self.expr_refs_xml(condition)
                    || self.loop_refs_xml(|path| path.body_refs_xml(body) || path.expr_refs_xml(condition))
            }
            StmtKind::For { init, condition, update, body } => {
                init.as_deref().is_some_and(|stmt| self.stmt_refs_xml(stmt))
                    || self.loop_refs_xml(|path| {
                        condition.as_ref().is_some_and(|expr| path.expr_refs_xml(expr))
                            || path.body_refs_xml(body)
                            || update.as_deref().is_some_and(|stmt| path.stmt_refs_xml(stmt))
                    })
            }
            StmtKind::ArrayAssign { index, value, .. } => {
                self.expr_refs_xml(index) || self.expr_refs_xml(value)
            }
            StmtKind::NestedArrayAssign { target, value } => {
                self.expr_refs_xml(target) || self.expr_refs_xml(value)
            }
            StmtKind::ArrayPush { value, .. } => self.expr_refs_xml(value),
            StmtKind::TypedAssign {
                type_expr, value, ..
            } => type_refs_xml(type_expr) || self.expr_refs_xml(value),
            StmtKind::Foreach { array, body, .. } => {
                self.expr_refs_xml(array) || self.loop_refs_xml(|path| {
                    path.dom.observe_statement(stmt);
                    path.body_refs_xml(body)
                })
            }
            StmtKind::Switch { subject, cases, default } => self.switch_refs_xml(subject, cases, default.as_deref()),
            StmtKind::Include { path, .. } => self.expr_refs_xml(path),
            StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::NamespaceBlock { body, .. } => body.iter().any(|value| self.stmt_refs_xml(value)),
            StmtKind::Try { try_body, catches, finally_body } => {
                let success = self.fork();
                if success.body_refs_xml(try_body) { return true; }
                let exceptional = self.fork();
                exceptional.dom.join(&success.dom);
                for catch in catches {
                    let branch = exceptional.fork();
                    if catch.exception_types.iter().any(name_is_xml_class) || branch.body_refs_xml(&catch.body) { return true; }
                    success.dom.join(&branch.dom);
                }
                self.dom.replace(&success.dom);
                finally_body.as_ref().is_some_and(|body| self.body_refs_xml(body))
            }
            StmtKind::FunctionDecl {
                params,
                variadic_type,
                return_type,
                body,
                ..
            } => {
                self.params_ref_xml(params)
                    || variadic_type.as_ref().is_some_and(type_refs_xml)
                    || return_type.as_ref().is_some_and(type_refs_xml)
                    || self.nested_body_refs_xml(params, body, false)
            }
            StmtKind::Return(value) => value.as_ref().is_some_and(|value| self.expr_refs_xml(value)),
            StmtKind::ConstDecl { value, .. } => self.expr_refs_xml(value),
            StmtKind::ListUnpack { value, .. } => self.expr_refs_xml(value),
            StmtKind::StaticVar { init, .. } => self.expr_refs_xml(init),
            StmtKind::ClassDecl {
                name,
                extends,
                implements,
                trait_uses,
                properties,
                methods,
                constants,
                ..
            } => {
                extends.as_ref().is_some_and(name_is_xml_class)
                    || implements.iter().any(name_is_xml_class)
                    || trait_uses.iter().any(trait_use_refs_xml)
                    || properties.iter().any(|value| self.class_property_refs_xml(value))
                    || methods.iter().any(|value| Self { dom: self.dom.in_class(name) }.class_method_refs_xml(value))
                    || constants.iter().any(|value| self.class_const_refs_xml(value))
            }
            // An enum carries the same member kinds as a class (methods, constants, trait
            // uses, implemented interfaces) on top of its backing type and cases; a program
            // whose only xml call sits in an enum method must still inject the prelude.
            StmtKind::EnumDecl {
                name,
                backing_type,
                cases,
                implements,
                trait_uses,
                methods,
                constants,
                ..
            } => {
                backing_type.as_ref().is_some_and(type_refs_xml)
                    || cases.iter().any(|value| self.enum_case_refs_xml(value))
                    || implements.iter().any(name_is_xml_class)
                    || trait_uses.iter().any(trait_use_refs_xml)
                    || methods.iter().any(|value| Self { dom: self.dom.in_class(name) }.class_method_refs_xml(value))
                    || constants.iter().any(|value| self.class_const_refs_xml(value))
            }
            StmtKind::PackedClassDecl { fields, .. } => fields.iter().any(packed_field_refs_xml),
            StmtKind::InterfaceDecl {
                name,
                extends,
                properties,
                methods,
                constants,
                ..
            } => {
                extends.iter().any(name_is_xml_class)
                    || properties.iter().any(|value| self.class_property_refs_xml(value))
                    || methods.iter().any(|value| Self { dom: self.dom.in_class(name) }.class_method_refs_xml(value))
                    || constants.iter().any(|value| self.class_const_refs_xml(value))
            }
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods,
                constants,
                ..
            } => {
                trait_uses.iter().any(trait_use_refs_xml)
                    || properties.iter().any(|value| self.class_property_refs_xml(value))
                    || methods.iter().any(|value| Self { dom: self.dom.in_class(name) }.class_method_refs_xml(value))
                    || constants.iter().any(|value| self.class_const_refs_xml(value))
            }
            StmtKind::PropertyAssign { object, value, .. } => {
                self.expr_refs_xml(object) || self.expr_refs_xml(value)
            }
            StmtKind::StaticPropertyAssign {
                receiver, value, ..
            }
            | StmtKind::StaticPropertyArrayPush {
                receiver, value, ..
            } => receiver_refs_xml(receiver) || self.expr_refs_xml(value),
            StmtKind::StaticPropertyArrayAssign {
                receiver,
                index,
                value,
                ..
            } => receiver_refs_xml(receiver) || self.expr_refs_xml(index) || self.expr_refs_xml(value),
            StmtKind::PropertyArrayPush { object, value, .. } => {
                self.expr_refs_xml(object) || self.expr_refs_xml(value)
            }
            StmtKind::PropertyArrayAssign {
                object,
                index,
                value,
                ..
            } => self.expr_refs_xml(object) || self.expr_refs_xml(index) || self.expr_refs_xml(value),
        };
        self.dom.observe_statement(stmt);
        result
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the xml AST walk: every `xml_*` / `xmlwriter_*` call and every
    //! `XMLParser` / `XMLWriter` class-name position is detected across `\`-qualified and
    //! mixed-case spellings, while unrelated `xml`-shaped user names and non-call mentions
    //! are not.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests parse raw source (pre name-resolution), matching the stage at which
    //!   `program_uses_xml` runs inside `inject_if_used`.

    use super::*;

    /// Parses source the same way `inject_if_used` sees it: tokenize then parse, before
    /// any name resolution.
    fn parse(source: &str) -> Vec<Stmt> {
        let tokens = crate::lexer::tokenize(source).expect("test source must tokenize");
        crate::parser::parse(&tokens).expect("test source must parse")
    }

    /// The catalog names both modules' functions and nothing else.
    #[test]
    fn the_function_set_is_the_catalog() {
        assert_eq!(xml_functions().len(), 67);
        assert!(xml_functions().contains(&"xml_parse_into_struct"));
        assert!(xml_functions().contains(&"xmlwriter_flush"));
        assert_eq!(xml_classes(), &["XMLParser", "XMLWriter", "SimpleXMLElement", "SimpleXMLIterator"]);
    }

    /// Every enumerated function is detected on its own, so a helper that only consumes
    /// a parser passed in still pulls the prelude.
    #[test]
    fn detects_every_enumerated_xml_function() {
        for name in xml_functions() {
            let source = format!("<?php function f($a, $b, $c) {{ return {name}($a, $b, $c); }}");
            assert!(
                program_uses_xml(&parse(&source)),
                "{name} must trigger xml prelude injection"
            );
        }
    }

    /// Both classes are detected in a parameter type hint, even when the body never
    /// calls an xml function.
    #[test]
    fn detects_every_class_type_hint() {
        for class in xml_classes() {
            let source = format!("<?php function f({class} $c): bool {{ return true; }}");
            assert!(
                program_uses_xml(&parse(&source)),
                "{class} must trigger xml prelude injection"
            );
        }
    }

    /// `new XMLWriter()`, the static constructors and `instanceof` are class-name positions.
    #[test]
    fn detects_class_name_positions() {
        assert!(program_uses_xml(&parse("<?php $w = new XMLWriter();")));
        assert!(program_uses_xml(&parse("<?php $w = XMLWriter::toMemory();")));
        assert!(program_uses_xml(&parse(
            "<?php if ($x instanceof XMLParser) { echo 1; }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php class W extends XMLWriter {}"
        )));
    }

    /// Fully-qualified global functions/classes remain case-insensitive.
    #[test]
    fn detects_fully_qualified_and_case_insensitive() {
        assert!(program_uses_xml(&parse("<?php $p = \\XML_PARSER_CREATE();")));
        assert!(program_uses_xml(&parse(
            "<?php function f(\\xmlwriter $w): bool { return true; }"
        )));
    }

    /// An aliased import is detected through the import name.
    #[test]
    fn detects_aliased_use_import() {
        assert!(program_uses_xml(&parse(
            "<?php use XMLWriter as W; function f(W $w): bool { return true; }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php use function xml_parse as p; $r = p($x, $d);"
        )));
    }

    /// An enum method body is a call position like a class method's.
    #[test]
    fn detects_enum_methods() {
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { case Xml; public function parse(string $d): int { $p = xml_parser_create(); return xml_parse($p, $d, true); } }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { case Xml; public static function make(): XMLWriter { return new XMLWriter(); } }"
        )));
    }

    /// An enum constant initializer naming a xml class is a class-name position.
    #[test]
    fn detects_enum_constants() {
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { case Xml; const WRITER = XMLWriter::class; }"
        )));
    }

    /// An enum's implemented interfaces and trait uses are walked like a class's.
    #[test]
    fn detects_enum_implements_and_trait_uses() {
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt implements XMLParser { case Xml; }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php enum Fmt { use XMLWriter; case Xml; }"
        )));
    }

    /// An enum without any xml reference in its members does not inject.
    #[test]
    fn ignores_unrelated_enum() {
        assert!(!program_uses_xml(&parse(
            "<?php enum Fmt: string { case Xml = 'xml'; const LABEL = 'x'; public function label(): string { return $this->value; } }"
        )));
    }

    /// A typed variadic parameter is a class-name position on a function, a method and a
    /// closure alike; a scalar element type is not.
    #[test]
    fn detects_variadic_parameter_types() {
        assert!(program_uses_xml(&parse(
            "<?php function f(XMLParser ...$parsers): int { return count($parsers); }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php class C { public function m(XMLWriter ...$writers): int { return count($writers); } }"
        )));
        assert!(program_uses_xml(&parse(
            "<?php $f = function (XMLParser ...$parsers): int { return count($parsers); };"
        )));
        assert!(!program_uses_xml(&parse(
            "<?php function f(int ...$xs): int { return count($xs); } $g = function (string ...$s): int { return count($s); };"
        )));
    }

    /// An unqualified call inside a namespace falls back to the global function when no
    /// local declaration or function import claims the name.
    #[test]
    fn detects_namespaced_function_fallback() {
        assert!(program_uses_xml(&parse(
            "<?php namespace App; $p = xml_parser_create();"
        )));
    }

    /// Fully-qualified user symbols that merely end in xml names stay in their namespace.
    #[test]
    fn ignores_fully_qualified_user_symbols() {
        assert!(!program_uses_xml(&parse(
            "<?php $value = \\Some\\xml_parse($handle, $data);"
        )));
        assert!(!program_uses_xml(&parse(
            "<?php function f(\\Acme\\XMLWriter $w): bool { return true; }"
        )));
    }

    /// A user helper with an `xml_` prefix, a string mentioning a function, and an `XML_*`
    /// constant on its own do not inject the prelude.
    #[test]
    fn ignores_unrelated_programs() {
        assert!(!program_uses_xml(&parse(
            "<?php function xml_helper($x) { return $x; } echo xml_helper(1);"
        )));
        assert!(!program_uses_xml(&parse(
            r#"<?php $s = "xml_parser_create"; echo $s;"#
        )));
        assert!(!program_uses_xml(&parse("<?php echo XML_ERROR_SYNTAX;")));
    }

    /// A user loadXML method stays independent of libxml2, including alongside HTML-only DOM use.
    #[test]
    fn ignores_unrelated_load_xml_receivers() {
        for source in [
            "class Config { function loadXML(): void {} } (new Config())->loadXML();",
            "class Config { function loadXML(): void {} } $cfg = new Config(); $cfg->loadXML();",
            "class Config { function loadXML(): void {} } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); $cfg = new Config(); $cfg->loadXML();",
            "class Config { function loadXML(): void {} } function run(Config $cfg): void { $cfg->loadXML(); } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); run(new Config());",
            "namespace App; class DOMDocument { function loadXML(): void {} } (new DOMDocument())->loadXML();",
            "class Config extends stdClass { function loadXML(): void {} } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); (new Config())->loadXML();",
            "class Config { function loadXML(): void {} } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); $doc = new Config(); $doc->loadXML();",
            "class Config { function loadXML(): void {} } $doc = new DOMDocument(); $change = function (&$value): void { $value = new DOMDocument(); }; $change($doc); $doc = new Config(); $doc->loadXML();",
            "class Config { function loadXML(): void {} } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); $cfg = new Config(); var_dump($cfg); $cfg->loadXML();",
            "class Config { function loadXML($value): void {} } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); $cfg = new Config(); $cfg->loadXML($cfg);",
            "if ($argc) { class Config { function loadXML(): void {} } } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); $cfg = new Config(); $cfg->loadXML();",
            "class Config { function loadXML(): void {} } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); (new Config())->{'loadXML'}();",
            "class Config { function loadXML(): void {} } $doc = new DOMDocument(); $doc->loadHTML('<p/>'); call_user_func([new Config(), 'loadXML']);",
            "class Config { function loadXML(): void {} } function readValue($value): void {} $doc = new DOMDocument(); $cfg = new Config(); readValue(value: $cfg); $cfg->loadXML();",
        ] {
            assert!(!program_uses_xml(&parse(&format!("<?php {source}"))), "{source}");
        }
    }

    /// Real DOM receivers retain automatic XML support across aliases, subclasses, and callbacks.
    #[test]
    fn detects_dom_load_xml_receivers() {
        for source in [
            "(new DOMDocument())->loadXML('<r/>');",
            "$doc = new DOMDocument(); $alias = $doc; $alias->loadXML('<r/>');",
            "function run(DOMDocument $doc): void { $doc->loadXML('<r/>'); }",
            "class Document extends DOMDocument { function parse(): void { $this->loadXML('<r/>'); } }",
            "class Holder { public DOMDocument $doc; function parse(): void { $this->doc->loadXML('<r/>'); } }",
            "$doc = new DOMDocument(); $parse = $doc->loadXML(...); $parse('<r/>');",
            "$doc = new DOMDocument(); $parse = function () use ($doc) { $doc->loadXML('<r/>'); };",
            "class Config {} $doc = new Config(); if ($argc) { $doc = new DOMDocument(); } $doc->loadXML('<r/>');",
            "namespace App; use DOMDocument as Doc; $doc = new Doc(); $doc->loadXML('<r/>');",
            "class Config {} $doc = new Config(); while ($argc) { $doc->loadXML('<r/>'); $doc = new DOMDocument(); }",
            "function run(): void { $doc = new Config(); while ($argc) { $doc->loadXML('<r/>'); $doc = new DOMDocument(); } } class Config {}",
            "if ($argc) { class Document extends DOMDocument {} } $doc = new Document(); $doc->loadXML('<r/>');",
            "class Config {} $doc = new Config(); $change = function () use (&$doc): void { $doc = new DOMDocument(); }; $change(); $doc->loadXML('<r/>');",
            "class Config {} $doc = new Config(); $change = function (&$value): void { $value = new DOMDocument(); }; $change($doc); $doc->loadXML('<r/>');",
            "$doc = new DOMDocument(); $doc->{'loadXML'}('<r/>');",
            "$doc = new DOMDocument(); $method = 'loadXML'; $doc->$method('<r/>');",
            "$doc = new DOMDocument(); $doc?->{'loadXML'}('<r/>');",
            "$doc = new DOMDocument(); call_user_func([$doc, 'loadXML'], '<r/>');",
            "$doc = new DOMDocument(); call_user_func_array([$doc, 'loadXML'], ['<r/>']);",
            "class Config {} $doc = new DOMDocument(); if ($argc) { $doc = new Config(); } $doc->loadXML('<r/>');",
            "class Config {} $doc = new Config(); $change = function () use (&$doc): void { $doc = new DOMDocument(); }; $doc = new Config(); $change(); $doc->loadXML('<r/>');",
            "class Config {} function change(&$value): void { $value = new DOMDocument(); } $doc = new Config(); change(value: $doc); $doc->loadXML('<r/>');",
            "class Config {} $doc = new DOMDocument(); $argc && ($doc = new Config()); $doc->loadXML('<r/>');",
        ] {
            assert!(program_uses_xml(&parse(&format!("<?php {source}"))), "{source}");
        }
    }
}
