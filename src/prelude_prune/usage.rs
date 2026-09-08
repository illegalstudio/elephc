//! Purpose:
//! Collects the SYMBOL references and dynamic-dispatch hazards of an AST — the functions,
//! classes and methods it can reach, the string literals it carries, and whether it does
//! anything the walk cannot see through.
//!
//! Called from:
//! - `crate::prelude_prune::prune`, which turns this into a kept/dropped decision.
//! - `crate::web_prelude::inject_if_web()` for its pay-for-use declaration selection.
//!
//! Key details:
//! - Literal `function_exists`, `is_callable`, `class_exists` and callback names count as
//!   references. That matters more than it looks: a guard whose subject gets pruned takes its
//!   ELSE branch silently, which is the one failure mode with no diagnostic.
//! - A METHOD name is recorded without a receiver type, because this walk runs before name
//!   resolution. The pruner therefore roots every class carrying that method — an
//!   over-approximation by construction, never an under-approximation.
//! - Every string literal is recorded. It is the fallback root set for a program that dispatches
//!   dynamically: instead of surrendering and keeping the whole surface, the pruner keeps the
//!   symbols the program actually names.
//!
//! THE CONTRACT THIS MODULE OWES ITS CALLER. A reachability pass is only as safe as the set of
//! channels its walk knows about, so those channels are enumerated here rather than discovered
//! one incident at a time:
//!
//! - NORMALIZATION. Every recorded name is `php_symbol_key`-folded with any leading `\` stripped,
//!   because PHP symbol names are case-insensitive and the pruner's index is keyed the same way.
//!   A raw name would silently fail to match.
//! - THE PROBE SET (`FUNCTION_PROBES`, `CLASS_PROBES`) — builtins that NAME a symbol and answer
//!   quietly when it is absent. Their subject is a reference; a probe on a name this walk cannot
//!   read is introspection, because its wrong answer is silent. `CLASS_PROBES` carries WHICH
//!   ARGUMENT names the class, because it is not always the first: `is_a($obj, 'Imagick')` names
//!   its class second, and reading position zero there is how the one name that had to survive
//!   gets recorded as a method instead.
//! - THE CALLABLE SET (`CALLABLE_TAKING_BUILTINS`) — builtins that will INVOKE a string. A literal
//!   there is a plain reference and does not depend on harvest mode:
//!   `register_shutdown_function('session_write_close')` contains no dynamic call at all.
//! - THE ENUMERATION SET (`INTROSPECTION_BUILTINS`) — builtins that hand over the whole symbol
//!   table and name nothing.
//! - IMPORTS. `use function imagecreate as ic;` names `imagecreate`, and this walk runs before
//!   name resolution, so nothing later will connect `ic()` back to it.
//!
//! Adding a prelude to `crate::prelude_prune` means checking this list against how that prelude's
//! declarations can be reached — including by a compiler pass that SYNTHESISES a call to one.

use std::collections::HashSet;

use crate::names::php_symbol_key;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, InstanceOfTarget,
    Stmt, StmtKind,
};

/// Symbol-reference summary for one AST subtree.
#[derive(Clone, Debug, Default)]
pub(crate) struct Usage {
    /// Free-function names, normalized case-insensitively.
    pub(crate) functions: HashSet<String>,
    /// Class names, normalized case-insensitively: `new C`, `C::`, `instanceof C`, a type hint,
    /// a `catch` type, an `extends`/`implements`, or a literal `class_exists('C')`.
    pub(crate) classes: HashSet<String>,
    /// Method names seen on a receiver whose class this walk cannot know.
    pub(crate) methods: HashSet<String>,
    /// Every string literal in the subtree, normalized. The fallback root set.
    pub(crate) literals: HashSet<String>,
    /// PHP variable names written exactly as they appear (`$_SERVER` → `_SERVER`),
    /// case-sensitively, since PHP variables are. Only names the source SPELLS land
    /// here: a `$$computed` access does not, which is what keeps a consumer honest
    /// about being a pay-for-use gate rather than a completeness claim.
    pub(crate) variables: HashSet<String>,
    /// A CALL whose target this walk cannot name (`$f()`, `call_user_func($x)`, a closure
    /// call). Its failure mode is LOUD — an undefined function at the call site — so the pruner
    /// answers it by widening the roots to the names the program does mention.
    pub(crate) dynamic_function_call: bool,
    /// The program contains a `yield` or `yield from`, which makes its enclosing function a
    /// generator and materializes a `Generator` object no source line names.
    pub(crate) uses_yield: bool,
    /// A `new $c` whose class name this walk cannot resolve to a literal.
    ///
    /// `codegen_support::dynamic_new::supported_dynamic_new_builtin_class_names` lists the
    /// builtin classes such a site can construct, and it includes every builtin throwable — so a
    /// gate that decides which of them to register has to treat this as "any of them". A literal
    /// name lands in `literals` and needs no such widening; this flag is for the rest.
    pub(crate) constructs_dynamic_class: bool,
    /// The program asks a question this walk cannot answer and whose WRONG ANSWER IS SILENT:
    /// it enumerates the symbol table (`get_defined_functions`, `eval`), or it PROBES a computed
    /// name (`function_exists($f)`). A dynamic CALL that is wrong fails loudly at the call site;
    /// a dynamic PROBE that is wrong just takes the guard's else branch and the program does
    /// something different for the rest of its life. Nothing approximates that, so it disables
    /// pruning outright.
    pub(crate) introspects: bool,
}

impl Usage {

    /// Returns true when a PHP function is referenced case-insensitively.
    pub(crate) fn references(&self, name: &str) -> bool {
        self.functions.contains(&php_symbol_key(name))
    }
}

/// PHP builtins that hand the whole symbol table to the program. None of them names what it
/// will reach, so seeing one means the prelude must be kept whole.
const INTROSPECTION_BUILTINS: &[&str] = &[
    "eval",
    "get_defined_functions",
    "get_declared_classes",
    "get_declared_interfaces",
    "get_declared_traits",
    "spl_autoload_register",
];

/// Builtins whose FIRST argument names a FUNCTION and is answered silently when it is missing.
///
/// A probe is not a call. `function_exists('imagecreatefromwebp')` returning `false` because the
/// pruner dropped the subject does not fail — it takes the guard's else branch, and the program
/// quietly does something else forever. So the subject is a reference, and a probe on a name
/// this walk cannot read is INTROSPECTION, not a mere dynamic call.
const FUNCTION_PROBES: &[&str] = &["function_exists", "is_callable"];

/// The same, for builtins whose first argument names a CLASS, INTERFACE or TRAIT.
/// One class probe, and WHICH ARGUMENT of it names the class.
///
/// The position is not decoration. `is_a($obj, 'Imagick')` names its class SECOND — the first
/// argument is the subject being tested. A table that assumes position zero reads `$obj`, finds
/// no literal, and keeps the whole surface: safe, but it throws the pass away for a construct
/// that was perfectly readable. Worse, on `is_a('Foo', 'Imagick')` it recorded `Foo` as the class
/// and `Imagick` as a MEMBER, so the one name that actually had to survive was rooted nowhere and
/// the probe would have answered false in silence.
///
/// `member` is the index of an argument naming a method or property, where the builtin has one.
struct ClassProbe {
    name: &'static str,
    class: usize,
    member: Option<usize>,
}

const fn probe(name: &'static str, class: usize, member: Option<usize>) -> ClassProbe {
    ClassProbe {
        name,
        class,
        member,
    }
}

const CLASS_PROBES: &[ClassProbe] = &[
    probe("class_exists", 0, None),
    probe("interface_exists", 0, None),
    probe("trait_exists", 0, None),
    probe("enum_exists", 0, None),
    probe("method_exists", 0, Some(1)),
    probe("property_exists", 0, Some(1)),
    probe("get_class_methods", 0, None),
    probe("get_class_vars", 0, None),
    probe("get_parent_class", 0, None),
    probe("class_implements", 0, None),
    probe("class_parents", 0, None),
    probe("class_uses", 0, None),
    probe("is_a", 1, None),
    probe("is_subclass_of", 1, None),
];

/// Builtins that take a CALLABLE and will invoke it. A literal argument here is a plain
/// reference — the string is statically visible and unconditionally used as a symbol — so it
/// belongs in `functions`, not in the `literals` pool that only a dynamic call unlocks.
///
/// `register_shutdown_function('session_write_close')` is idiomatic PHP and contains no dynamic
/// call at all. Treating it as a hazard rather than a reference is how a shutdown handler goes
/// silently uninstalled.
///
/// The position matters: `usort($a, 'cmp')` names its callable SECOND. Every argument is
/// therefore examined, which over-approximates (a literal that merely looks like a function name
/// roots a declaration) in the only safe direction.
const CALLABLE_TAKING_BUILTINS: &[&str] = &[
    "array_filter",
    "array_map",
    "array_reduce",
    "array_walk",
    "array_walk_recursive",
    "call_user_func",
    "call_user_func_array",
    "class_alias",
    "header_register_callback",
    "iterator_apply",
    "ob_start",
    "preg_replace_callback",
    "preg_replace_callback_array",
    "register_shutdown_function",
    "register_tick_function",
    "session_set_save_handler",
    "set_error_handler",
    "set_exception_handler",
    "uasort",
    "uksort",
    "usort",
];

/// Records one class name.
fn record_class(usage: &mut Usage, name: &str) {
    let name = name.trim_start_matches('\\');
    if !name.is_empty() {
        usage.classes.insert(php_symbol_key(name));
    }
}

/// Records one method name seen without a known receiver class.
fn record_method(usage: &mut Usage, name: &str) {
    usage.methods.insert(php_symbol_key(name));
}

/// Records a literal callable, in every form PHP accepts one, and reports whether it was READ.
///
/// `'strlen'` names a function; `'Imagick::clear'` names a class AND a method, and is legal
/// wherever a callable is; `['Imagick', 'clear']` and `[$obj, 'clear']` are the array forms,
/// whose second element names the method. `[$obj, 'clear']` counts as read even though the
/// receiver is opaque: the method name roots every class declaring it, which is the same
/// approximation `$obj->clear()` gets.
///
/// THE RETURN VALUE IS THE POINT. `$obj->$m()` desugars to `call_user_func([$obj, $m], …)`, so it
/// arrives wearing the very array literal that `['Imagick', 'clear']` wears. Judging the argument
/// by its SHAPE calls both analysable; judging it by what could be read separates them.
fn record_literal_callable(usage: &mut Usage, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(text) => {
            if let Some((class, method)) = text.split_once("::") {
                record_class(usage, class);
                record_method(usage, method);
                true
            } else if text.is_empty() {
                false
            } else {
                record_name(usage, text);
                true
            }
        }
        ExprKind::ArrayLiteral(items) => match items.as_slice() {
            [receiver, method] => {
                if let ExprKind::StringLiteral(name) = &receiver.kind {
                    record_class(usage, name);
                }
                match &method.kind {
                    ExprKind::StringLiteral(name) => {
                        record_method(usage, name);
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        },
        _ => false,
    }
}

/// Collects direct and literal-indirect function references from a program.
pub(crate) fn collect(program: &[Stmt]) -> Usage {
    let mut usage = Usage::default();
    for stmt in program {
        scan_stmt(stmt, &mut usage);
    }
    usage
}



/// Records one normalized PHP function name.
fn record_name(usage: &mut Usage, name: &str) {
    usage
        .functions
        .insert(php_symbol_key(name.trim_start_matches('\\')));
}

/// Records a literal callback/probe target when it denotes a free function.
fn record_literal_function(usage: &mut Usage, expr: Option<&Expr>) {
    let Some(Expr {
        kind: ExprKind::StringLiteral(name),
        ..
    }) = expr
    else {
        return;
    };
    if !name.contains("::") && !name.is_empty() {
        record_name(usage, name);
    }
}

/// Scans parameter default expressions.
fn scan_params(
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    usage: &mut Usage,
) {
    for (_, ty, default, _) in params {
        if let Some(ty) = ty {
            scan_type(ty, usage);
        }
        if let Some(default) = default {
            scan_expr(default, usage);
        }
    }
}

/// Records every class named by a type expression.
///
/// A type hint is a reference: a kept method whose parameter is typed `ImagickPixel` needs that
/// class to exist, so the closure has to follow hints as well as call sites.
fn scan_type(ty: &crate::parser::ast::TypeExpr, usage: &mut Usage) {
    use crate::parser::ast::TypeExpr;
    match ty {
        TypeExpr::Named(name) => record_class(usage, &name.as_canonical()),
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            scan_type(inner, usage)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            for member in members {
                scan_type(member, usage);
            }
        }
        TypeExpr::Ptr(Some(name)) => record_class(usage, &name.as_canonical()),
        _ => {}
    }
}

/// Scans one class property's type and initializer.
fn scan_property(property: &ClassProperty, usage: &mut Usage) {
    if let Some(ty) = &property.type_expr {
        scan_type(ty, usage);
    }
    if let Some(default) = &property.default {
        scan_expr(default, usage);
    }
}

/// Scans one class-like method body, parameter types and defaults.
fn scan_method(method: &ClassMethod, usage: &mut Usage) {
    scan_params(&method.params, usage);
    if let Some(ty) = &method.return_type {
        scan_type(ty, usage);
    }
    scan_program(&method.body, usage);
}

/// Scans one class constant initializer.
fn scan_class_const(constant: &ClassConst, usage: &mut Usage) {
    scan_expr(&constant.value, usage);
}

/// Scans a statement list into an existing summary.
fn scan_program(program: &[Stmt], usage: &mut Usage) {
    for stmt in program {
        scan_stmt(stmt, usage);
    }
}

/// Scans every expression-bearing position of one statement.
fn scan_stmt(stmt: &Stmt, usage: &mut Usage) {
    match &stmt.kind {
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::Assign { value: expr, .. }
        | StmtKind::TypedAssign { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::ListUnpack { value: expr, .. }
        | StmtKind::Return(Some(expr))
        | StmtKind::ArrayPush { value: expr, .. }
        | StmtKind::StaticPropertyAssign { value: expr, .. }
        | StmtKind::StaticPropertyArrayPush { value: expr, .. }
        | StmtKind::Include { path: expr, .. } => scan_expr(expr, usage),
        StmtKind::RefAssign { source, .. } => scan_expr(source, usage),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            scan_expr(object, usage);
            scan_expr(value, usage);
        }
        StmtKind::DynamicPropertyArrayPush {
            object,
            property,
            value,
        } => {
            scan_expr(object, usage);
            scan_expr(property, usage);
            scan_expr(value, usage);
        }
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            scan_expr(object, usage);
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            scan_expr(target, usage);
            scan_expr(value, usage);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            scan_expr(condition, usage);
            scan_program(then_body, usage);
            for (condition, body) in elseif_clauses {
                scan_expr(condition, usage);
                scan_program(body, usage);
            }
            if let Some(body) = else_body {
                scan_program(body, usage);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            scan_program(then_body, usage);
            if let Some(body) = else_body {
                scan_program(body, usage);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            scan_expr(condition, usage);
            scan_program(body, usage);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                scan_stmt(init, usage);
            }
            if let Some(condition) = condition {
                scan_expr(condition, usage);
            }
            if let Some(update) = update {
                scan_stmt(update, usage);
            }
            scan_program(body, usage);
        }
        StmtKind::Foreach { array, body, .. } => {
            scan_expr(array, usage);
            scan_program(body, usage);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            scan_expr(subject, usage);
            for (patterns, body) in cases {
                for pattern in patterns {
                    scan_expr(pattern, usage);
                }
                scan_program(body, usage);
            }
            if let Some(body) = default {
                scan_program(body, usage);
            }
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => scan_program(body, usage),
        StmtKind::FunctionDecl {
            params,
            return_type,
            body,
            ..
        } => {
            scan_params(params, usage);
            if let Some(ty) = return_type {
                scan_type(ty, usage);
            }
            scan_program(body, usage);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            scan_program(try_body, usage);
            for catch in catches {
                // `catch (ImagickException $e)` names the class as surely as `new` does.
                for exception in &catch.exception_types {
                    record_class(usage, &exception.as_canonical());
                }
                scan_program(&catch.body, usage);
            }
            if let Some(body) = finally_body {
                scan_program(body, usage);
            }
        }
        StmtKind::ClassDecl {
            extends,
            implements,
            properties,
            methods,
            constants,
            ..
        } => {
            // A supertype is a reference: keeping a class means keeping what it inherits from.
            if let Some(parent) = extends {
                record_class(usage, &parent.as_canonical());
            }
            for interface in implements {
                record_class(usage, &interface.as_canonical());
            }
            for property in properties {
                scan_property(property, usage);
            }
            for method in methods {
                scan_method(method, usage);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        StmtKind::TraitDecl {
            properties,
            methods,
            constants,
            ..
        }
        | StmtKind::InterfaceDecl {
            properties,
            methods,
            constants,
            ..
        } => {
            for property in properties {
                scan_property(property, usage);
            }
            for method in methods {
                scan_method(method, usage);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        StmtKind::EnumDecl { cases, methods, constants, .. } => {
            for case in cases {
                if let Some(value) = &case.value {
                    scan_expr(value, usage);
                }
            }
            for method in methods {
                scan_method(method, usage);
            }
            for constant in constants {
                scan_class_const(constant, usage);
            }
        }
        // `use function imagecreate as ic;` names `imagecreate`, and this walk runs BEFORE name
        // resolution — so by the time anything could resolve `ic()` back to it, the pruner has
        // already decided. The IMPORT is the reference, whichever kind it is.
        StmtKind::UseDecl { imports } => {
            for import in imports {
                let name = import.name.as_canonical();
                record_name(usage, &name);
                record_class(usage, &name);
            }
        }
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::Global { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

/// Scans every child and callable position of one expression.
fn scan_expr(expr: &Expr, usage: &mut Usage) {
    match &expr.kind {
        ExprKind::IncludeValue { path, .. } => scan_expr(path, usage),
        ExprKind::FunctionCall { name, args } => {
            let name = php_symbol_key(name.as_str().trim_start_matches('\\'));
            record_name(usage, &name);
            let literal_first = matches!(
                args.first().map(|arg| &arg.kind),
                Some(ExprKind::StringLiteral(_))
            );
            if FUNCTION_PROBES.contains(&name.as_str()) {
                // A PROBE on a name this walk cannot read fails SILENTLY when it is wrong, so it
                // is introspection rather than a mere dynamic call.
                if literal_first {
                    record_literal_function(usage, args.first());
                } else {
                    usage.introspects = true;
                }
            } else if let Some(probe) = CLASS_PROBES.iter().find(|p| p.name == name) {
                // Read the argument that actually names the class, not whichever came first.
                if let Some(Expr {
                    kind: ExprKind::StringLiteral(class),
                    ..
                }) = args.get(probe.class)
                {
                    record_class(usage, class);
                    // `method_exists('C', 'm')` and friends name the member too.
                    if let Some(Expr {
                        kind: ExprKind::StringLiteral(member),
                        ..
                    }) = probe.member.and_then(|index| args.get(index))
                    {
                        record_method(usage, member);
                    }
                } else {
                    // The class it asks about cannot be read, and a wrong answer is silent.
                    usage.introspects = true;
                }
                // `is_a`/`is_subclass_of` accept a class NAME as their subject as well, so that
                // argument names a class too when it is a literal.
                if probe.class != 0 {
                    if let Some(Expr {
                        kind: ExprKind::StringLiteral(subject),
                        ..
                    }) = args.first()
                    {
                        record_class(usage, subject);
                    }
                }
            } else if CALLABLE_TAKING_BUILTINS.contains(&name.as_str()) {
                // A literal callable is a plain reference; only one this walk cannot READ is a
                // hazard. Every argument is examined because the position varies — `usort($a,
                // 'cmp')` names its callable second — and `fold` rather than `any` so that
                // examining does not stop early: recording the names is the point, not the flag.
                let read = args.iter().fold(false, |seen, arg| {
                    record_literal_callable(usage, arg) || seen
                });
                // An argumentless `ob_start()` installs no callback and is not a hazard; one whose
                // arguments exist but say nothing — `usort($a, $cmp)`, `call_user_func($x)` — is.
                if !read && !args.is_empty() {
                    usage.dynamic_function_call = true;
                }
            } else if INTROSPECTION_BUILTINS.contains(&name.as_str()) {
                usage.introspects = true;
            }
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            record_name(usage, name.as_str());
        }
        // A first-class callable ROOTS EXACTLY WHAT CALLING IT WOULD. `Imagick::queryFormats(...)`
        // names a class and a method just as `Imagick::queryFormats()` does; deferring the call
        // does not make the declaration unnecessary, it makes the reference easier to miss.
        //
        // These two arms used to root less than their calling counterparts a few lines below:
        // the static one recorded nothing at all, and the instance one scanned the receiver but
        // dropped the method name — which is the ONLY handle there is, since the receiver's
        // class is unknown before name resolution. Either way the pruner then deleted a
        // declaration the program goes on to use, and the failure surfaces as an undefined
        // class rather than as anything pointing back here.
        ExprKind::FirstClassCallable(CallableTarget::Method { object, method }) => {
            record_method(usage, method);
            scan_expr(object, usage);
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }) => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                record_class(usage, &name.as_canonical());
            }
            record_method(usage, method);
        }
        ExprKind::ExprCall { callee, args } => {
            usage.dynamic_function_call = true;
            scan_expr(callee, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::ClosureCall { args, .. } => {
            usage.dynamic_function_call = true;
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::BinaryOp { left, right, .. } => {
            scan_expr(left, usage);
            scan_expr(right, usage);
        }
        ExprKind::InstanceOf { value, target } => {
            scan_expr(value, usage);
            match target {
                InstanceOfTarget::Expr(target) => scan_expr(target, usage),
                InstanceOfTarget::Name(name) => record_class(usage, &name.as_canonical()),
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::PtrCast { expr: inner, .. } => scan_expr(inner, usage),
        ExprKind::YieldFrom(inner) => {
            usage.uses_yield = true;
            scan_expr(inner, usage);
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default }
        | ExprKind::Pipe {
            value,
            callable: default,
        } => {
            scan_expr(value, usage);
            scan_expr(default, usage);
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            prelude,
            ..
        } => {
            scan_expr(target, usage);
            scan_expr(value, usage);
            if let Some(result_target) = result_target {
                scan_expr(result_target, usage);
            }
            scan_program(prelude, usage);
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                scan_expr(item, usage);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                scan_expr(key, usage);
                scan_expr(value, usage);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            scan_expr(subject, usage);
            for (patterns, value) in arms {
                for pattern in patterns {
                    scan_expr(pattern, usage);
                }
                scan_expr(value, usage);
            }
            if let Some(default) = default {
                scan_expr(default, usage);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            scan_expr(array, usage);
            scan_expr(index, usage);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            scan_expr(condition, usage);
            scan_expr(then_expr, usage);
            scan_expr(else_expr, usage);
        }
        ExprKind::Closure { params, body, .. } => {
            scan_params(params, usage);
            scan_program(body, usage);
        }
        ExprKind::NamedArg { value, .. } => scan_expr(value, usage),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                record_class(usage, &name.as_canonical());
            }
            record_method(usage, method);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewObject { class_name, args } => {
            record_class(usage, &class_name.as_canonical());
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            usage.constructs_dynamic_class = true;
            scan_expr(name_expr, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            usage.constructs_dynamic_class = true;
            scan_expr(class_name, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => scan_expr(object, usage),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            scan_expr(object, usage);
            scan_expr(property, usage);
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        }
        | ExprKind::NullsafeMethodCall {
            object,
            method,
            args,
        } => {
            // The receiver's class is unknown before name resolution, so the METHOD NAME is the
            // only handle. The pruner roots every class that declares it.
            record_method(usage, method);
            scan_expr(object, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            // `$obj?->$m()` keeps its own node, where `$obj->$m()` desugars to `call_user_func`.
            // Only the nullsafe spelling reaches here, so the hazard is armed at both places or
            // at neither.
            if !record_literal_callable(usage, method) {
                usage.dynamic_function_call = true;
            }
            scan_expr(object, usage);
            scan_expr(method, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::BufferNew { len, .. } => scan_expr(len, usage),
        ExprKind::Yield { key, value } => {
            usage.uses_yield = true;
            if let Some(key) = key {
                scan_expr(key, usage);
            }
            if let Some(value) = value {
                scan_expr(value, usage);
            }
        }
        ExprKind::Variable(name) => {
            // A name repeats far more often than it first appears, and `collect` runs
            // five times per compile, so check membership before paying for the clone.
            if !usage.variables.contains(name.as_str()) {
                usage.variables.insert(name.clone());
            }
        }
        ExprKind::StringLiteral(text) => {
            usage.literals.insert(php_symbol_key(text.trim_start_matches('\\')));
        }
        ExprKind::ClassConstant { receiver, .. }
        | ExprKind::StaticPropertyAccess { receiver, .. }
        | ExprKind::ScopedConstantAccess { receiver, .. } => {
            if let crate::parser::ast::StaticReceiver::Named(name) = receiver {
                record_class(usage, &name.as_canonical());
            }
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::MagicConstant(_) => {}
        ExprKind::ObjectClassName { object } => scan_expr(object, usage),
    }
}
