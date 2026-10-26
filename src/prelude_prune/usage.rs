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
    /// Global constant names, case-sensitive, normalized without a leading separator.
    pub(crate) constants: HashSet<String>,
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
    /// Walk state, not a finding: true while the parsed source of a literal `eval` is being
    /// scanned, where an `include` resolves at runtime and names nothing this walk can read.
    pub(crate) scanning_eval_fragment: bool,
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

/// Constructs a builtin-class usage probe with an optional member identifier.
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
                let receiver_read = if let ExprKind::StringLiteral(name) = &receiver.kind {
                    record_class(usage, name);
                    true
                } else {
                    // Inline, `[$obj, 'm']` roots every class declaring `m`. Inside a literal
                    // fragment the interpreter resolves `[$c, 'fromArray']` against whatever
                    // builtin classes were registered, so an opaque receiver is a dynamic call.
                    !usage.scanning_eval_fragment
                };
                match &method.kind {
                    ExprKind::StringLiteral(name) => {
                        record_method(usage, name);
                        receiver_read
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



/// Scans the source of a literal `eval` as the inline code it will run as, returning false
/// when the argument is not a string literal or does not parse (the interpreter reports the
/// latter at runtime). The fragment's own names, literals and hazards land in `usage` exactly
/// as if the statements were spelled inline; a runtime `include` inside it is introspection.
fn scan_literal_eval_fragment(source: Option<&Expr>, usage: &mut Usage) -> bool {
    let Some(Expr {
        kind: ExprKind::StringLiteral(source),
        ..
    }) = source
    else {
        return false;
    };
    let Some(fragment) =
        crate::eval_aot::parse_literal_fragment(source, crate::strict_php::is_enabled())
    else {
        return false;
    };
    let outer = std::mem::replace(&mut usage.scanning_eval_fragment, true);
    // The fragment's own dynamic calls are judged on their own: the two flags are cleared for
    // the scan and restored afterwards, so inline code that already made a computed call does
    // not hide the one the fragment makes.
    let dynamic_call_before = std::mem::take(&mut usage.dynamic_function_call);
    let dynamic_class_before = std::mem::take(&mut usage.constructs_dynamic_class);
    scan_program(&fragment, usage);
    usage.scanning_eval_fragment = outer;
    // Inline, a computed call or `new $c` is answered by rooting the names the program spells,
    // because the compiled call site fails loudly when the pruner guessed wrong. The interpreter
    // resolves the same constructs against whatever the binary registered and reports a missing
    // builtin as if the program never had it, so inside a fragment they are introspection: the
    // whole surface stays registered, exactly as it did when every eval was opaque.
    if usage.dynamic_function_call || usage.constructs_dynamic_class {
        usage.introspects = true;
    }
    usage.dynamic_function_call |= dynamic_call_before;
    usage.constructs_dynamic_class |= dynamic_class_before;
    true
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
        // Both halves, so a prelude class named only in an `array<K, V>` key or value is not
        // pruned out from under the program that declared it.
        TypeExpr::AssocArray { key, value } => {
            scan_type(key, usage);
            scan_type(value, usage);
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
        | StmtKind::StaticPropertyArrayPush { value: expr, .. } => scan_expr(expr, usage),
        StmtKind::Include { path: expr, .. } => {
            // The resolver has already inlined every include it could read, so one still here
            // resolves at runtime. Inside a literal eval fragment that is the normal case, and the
            // included file's contents are exactly what this walk cannot see.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            scan_expr(expr, usage)
        }
        StmtKind::RefAssign { source, .. } => scan_expr(source, usage),
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            scan_expr(object, usage);
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
        StmtKind::NamespaceBlock { body, .. } => {
            // Names inside a namespaced fragment resolve through aliases this walk does not
            // perform, so such a fragment is introspection.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            scan_program(body, usage)
        }
        StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => scan_program(body, usage),
        StmtKind::FunctionDecl {
            name,
            params,
            return_type,
            body,
            ..
        } => {
            // A function the fragment declares keeps a builtin or native declaration of the
            // same name registered, so the interpreter can refuse the duplicate as PHP does.
            if usage.scanning_eval_fragment {
                record_name(usage, name);
            }
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
            name,
            extends,
            implements,
            properties,
            methods,
            constants,
            ..
        } => {
            // A class the fragment declares keeps a builtin of the same name registered, so the
            // interpreter can refuse the duplicate as PHP does (`eval('class SplStack {}')`).
            if usage.scanning_eval_fragment {
                record_class(usage, name);
            }
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
        StmtKind::InterfaceDecl {
            name,
            extends,
            properties,
            methods,
            constants,
            ..
        } => {
            // Same as a class declaration: the declared name keeps a builtin of that name, and
            // a parent interface is a reference (`interface Child extends DateTimeInterface`).
            if usage.scanning_eval_fragment {
                record_class(usage, name);
            }
            for parent in extends {
                record_class(usage, &parent.as_canonical());
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
            name,
            properties,
            methods,
            constants,
            ..
        } => {
            // Same as a class declaration: the declared name keeps a builtin of that name.
            if usage.scanning_eval_fragment {
                record_class(usage, name);
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
        StmtKind::EnumDecl { name, cases, methods, constants, .. } => {
            if usage.scanning_eval_fragment {
                record_class(usage, name);
            }
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
            // An import inside a fragment aliases names this walk cannot follow.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            for import in imports {
                let name = import.name.as_canonical();
                record_name(usage, &name);
                record_class(usage, &name);
            }
        }
        StmtKind::NamespaceDecl { .. } if usage.scanning_eval_fragment => {
            // A namespaced fragment resolves its names through aliases this walk does not perform.
            usage.introspects = true;
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
        ExprKind::IncludeValue { path, .. } => {
            // Like the include statement: a runtime include inside a fragment loads code this
            // walk cannot read.
            if usage.scanning_eval_fragment {
                usage.introspects = true;
            }
            scan_expr(path, usage)
        }
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
            } else if name == "eval" {
                // Only a runtime-sourced (or unparsable) eval hands the whole symbol table over;
                // a literal one is scanned as the inline code it is.
                if !scan_literal_eval_fragment(args.first(), usage) {
                    usage.introspects = true;
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
                InstanceOfTarget::Generic(class_type) => {
                    for name in class_type.named_classes() {
                        record_class(usage, &name.as_canonical());
                    }
                }
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
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries.iter() {
                for expr in entry.exprs() {
                    scan_expr(expr, usage);
                }
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
                let class = name.as_canonical();
                record_class(usage, &class);
                // `Closure::fromCallable('f')` invokes what it names, like `call_user_func`; one
                // whose argument this walk cannot read is a dynamic call.
                if php_symbol_key(&class) == "closure" && php_symbol_key(method) == "fromcallable" {
                    let read = args.iter().fold(false, |seen, arg| {
                        record_literal_callable(usage, arg) || seen
                    });
                    if !read && !args.is_empty() {
                        usage.dynamic_function_call = true;
                    }
                }
            }
            record_method(usage, method);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        // The head of the constructed type is the class this construction keeps alive; its
        // type arguments name classes of their own and are recorded the same way.
        ExprKind::NewGeneric { class_type, args } => {
            for name in class_type.named_classes() {
                record_class(usage, &name.as_canonical());
            }
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
        ExprKind::ConstRef(name) => {
            usage.constants.insert(
                name.as_canonical()
                    .trim_start_matches('\\')
                    .to_string(),
            );
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::This
        | ExprKind::MagicConstant(_) => {}
        ExprKind::ObjectClassName { object } => scan_expr(object, usage),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Program;

    /// Parses one tagged fixture without resolution or optimization passes.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// A literal eval contributes the names and literals of its source and is not introspection.
    #[test]
    fn literal_eval_is_collected_as_inline_code() {
        let usage = collect(&parse(
            "<?php eval('throw new DomainException(\"x\"); echo imagecreate(1, 1);');",
        ));
        assert!(!usage.introspects);
        assert!(!usage.scanning_eval_fragment);
        assert!(usage.classes.contains("domainexception"));
        assert!(usage.references("imagecreate"));
    }

    /// A runtime-sourced eval, an unparsable literal, and a runtime include inside a literal
    /// fragment are all introspection: nothing the walk can read names what they reach.
    #[test]
    fn opaque_evals_remain_introspection() {
        for source in [
            "<?php eval($argv[1]);",
            "<?php eval('not php at all');",
            "<?php eval('include \"part.php\";');",
        ] {
            let usage = collect(&parse(source));
            assert!(usage.introspects, "{source}");
            assert!(!usage.scanning_eval_fragment, "{source}");
        }
    }

    /// A computed call inside a literal fragment is introspection: the interpreter resolves it
    /// against whatever was registered, with no literal fallback roots to lean on.
    #[test]
    fn literal_eval_dynamic_call_is_introspection() {
        let usage = collect(&parse("<?php eval('$f = \"strlen\"; echo $f(\"x\");');"));
        assert!(usage.introspects);
        assert!(usage.dynamic_function_call);
    }

    /// A computed call outside the fragment stays a plain dynamic call even when a literal eval
    /// is present, so the fragment scan does not retroactively reclassify inline code.
    #[test]
    fn inline_dynamic_call_beside_a_literal_eval_stays_a_dynamic_call() {
        let usage = collect(&parse("<?php $f = $argv[1]; $f(); eval('echo 1;');"));
        assert!(!usage.introspects);
        assert!(usage.dynamic_function_call);
    }

    /// A computed call inside the fragment is introspection even when inline code already made
    /// one: the fragment is judged on its own calls, not on the flags it inherited.
    #[test]
    fn literal_eval_dynamic_call_after_an_inline_one_is_still_introspection() {
        let usage = collect(&parse(
            "<?php $f = 'strlen'; $f('x'); eval('$p = $argv[1]; echo $p($argv[2]) ? \"yes\" : \"no\";');",
        ));
        assert!(usage.introspects);
        assert!(usage.dynamic_function_call);
    }

    /// An array callable with an opaque receiver, `Closure::fromCallable` with a computed
    /// argument, and a runtime include expression are introspection inside a fragment, while
    /// the fully literal pair stays a plain reference.
    #[test]
    fn literal_eval_opaque_callables_and_include_values_are_introspection() {
        for source in [
            "<?php eval('class F {} $c = $argv[1]; $a = $c::fromArray([1]); echo count($a);');",
            "<?php eval('echo call_user_func([$argv[1], \"fromArray\"], [1]);');",
            "<?php eval('$c = Closure::fromCallable($argv[1]); echo $c();');",
            "<?php eval('$result = require \"part.php\"; echo $result;');",
        ] {
            let usage = collect(&parse(source));
            assert!(usage.introspects, "{source}");
        }
        let usage = collect(&parse("<?php eval('echo call_user_func([\"SplFixedArray\", \"fromArray\"], [1]);');"));
        assert!(!usage.introspects);
        assert!(usage.classes.contains("splfixedarray"));
        let usage = collect(&parse("<?php $o = new SplStack(); call_user_func([$o, 'push'], 1);"));
        assert!(!usage.introspects);
    }

    /// A function the fragment declares is recorded by name, so a builtin of that name stays
    /// registered for the interpreter to refuse the duplicate.
    #[test]
    fn literal_eval_declared_functions_are_recorded() {
        let usage = collect(&parse("<?php eval('function strlen(string $s): int { return 1; }');"));
        assert!(usage.references("strlen"));
        assert!(!usage.introspects);
    }

    /// A class, interface, trait or enum the fragment declares is recorded by name for the same
    /// reason, without turning the fragment into introspection.
    #[test]
    fn literal_eval_declared_class_likes_are_recorded() {
        let usage = collect(&parse(
            "<?php eval('class SplStack {} interface DateTimeInterface {} trait Fiber {} enum Generator {}');",
        ));
        for name in ["splstack", "datetimeinterface", "fiber", "generator"] {
            assert!(usage.classes.contains(name), "{name}");
        }
        assert!(!usage.introspects);
        let usage = collect(&parse("<?php eval('interface Child extends DateTimeInterface {}');"));
        assert!(usage.classes.contains("datetimeinterface"));
        assert!(!usage.introspects);
    }
}
