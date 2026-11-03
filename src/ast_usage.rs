//! Purpose:
//! Collects function references and dynamic-call hazards from a parsed AST.
//! Shared by the pay-for-use prelude injectors (`crate::web_prelude`,
//! `crate::parse_ini_prelude`, `crate::filter_var_prelude`) so each decides
//! whether to inject its elephc-PHP standard-library helper from the SAME
//! battle-tested, exhaustive AST walk instead of duplicating a per-prelude
//! scanner.
//!
//! Called from:
//! - `crate::web_prelude::inject_if_web()` before the web prelude is combined with user code.
//! - `crate::parse_ini_prelude::inject_if_used()` / `crate::filter_var_prelude::inject_if_used()`.
//!
//! Key details:
//! - Literal `function_exists`, `is_callable`, and callback names count as references.
//! - Unknown dynamic calls conservatively disable function pruning.
//! - The statement/expression matches are exhaustive with NO wildcard arm, so a new
//!   AST node kind is a compile error here rather than a silently-missed reference.

use std::collections::HashSet;

use crate::names::php_symbol_key;
use crate::parser::ast::{
    CallableTarget, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, InstanceOfTarget,
    Program, StaticReceiver, Stmt, StmtKind,
};

/// Function-reference summary for one AST subtree.
#[derive(Clone, Debug, Default)]
pub(crate) struct Usage {
    pub(crate) functions: HashSet<String>,
    /// Canonical class names reached by statically-named object construction.
    pub(crate) classes: HashSet<String>,
    pub(crate) dynamic_function_call: bool,
    /// Whether this subtree can load another PHP file whose references are not visible yet.
    /// Prelude pruning runs before include/autoload resolution, so includes must be treated as
    /// an unknown-code boundary until the resolver has expanded them.
    pub(crate) dynamic_code_load: bool,
    /// Whether an include sits in VALUE position (`return include $p;`). Only the resolver
    /// expands that node, so a subtree parsed after resolution still carries it.
    pub(crate) value_include: bool,
    /// Whether this subtree reads the runtime called class through a `static::` receiver.
    pub(crate) uses_late_static: bool,
    /// Global names the program reaches through `$GLOBALS['name']`, without the
    /// alias spelling — `$GLOBALS['app']` contributes `app`.
    ///
    /// IR lowering needs this to keep a top-level `$app` and a function's
    /// `$GLOBALS['app']` on ONE `_eir_global_app` slot; without it the top-level
    /// write lands in a `main` frame slot the function never reads.
    pub(crate) globals_keys: HashSet<String>,
    /// Variables stored into an array as a reference (`$a[$k] = &$v`, `[&$v]`), which must
    /// then BE a shareable reference cell rather than an alias of some other storage.
    pub(crate) array_reference_sources: HashSet<String>,
    /// Whether a `define()` names its constant with anything but a string literal, which needs
    /// the run-time constant table (`crate::dynamic_define_prelude`).
    pub(crate) dynamic_define: bool,
    /// Variables some closure captures by reference (`use (&$x)`) and writes through.
    pub(crate) ref_capture_writes: HashSet<String>,
    /// Whether the subtree hands a reference to one of its locals to storage another callee can
    /// reach: a `use (&$x)` capture, `$a = &$x`, `[&$x]`, `$o->p = &$x`, a by-reference
    /// `foreach`, or a `global` binding.
    pub(crate) exposes_locals: bool,
    /// Statically-named constructions (`new Foo($this->items, ...)`) that pass a PLACE -- a
    /// property, static property or array element -- as an argument, with where each one sits.
    /// Whether that argument reaches a by-reference parameter depends on `Foo`'s constructor,
    /// which only a pass that sees the class declarations can answer.
    pub(crate) place_constructions: Vec<PlaceConstruction>,
}

/// One `new Class(args)` whose arguments include places (see `Usage::place_constructions`).
#[derive(Clone, Debug)]
pub(crate) struct PlaceConstruction {
    /// Case-folded class name without the leading backslash.
    pub(crate) class_key: String,
    /// Positional index (`name` is `None`) or parameter name of each place argument.
    pub(crate) places: Vec<PlaceArgument>,
}

/// One place argument of a construction.
#[derive(Clone, Debug)]
pub(crate) struct PlaceArgument {
    pub(crate) position: usize,
    pub(crate) name: Option<String>,
}

impl Usage {
    /// Merges another subtree summary into this one.
    pub(crate) fn merge(&mut self, other: Self) {
        self.functions.extend(other.functions);
        self.classes.extend(other.classes);
        self.dynamic_function_call |= other.dynamic_function_call;
        self.dynamic_code_load |= other.dynamic_code_load;
        self.value_include |= other.value_include;
        self.uses_late_static |= other.uses_late_static;
        self.globals_keys.extend(other.globals_keys);
        self.array_reference_sources
            .extend(other.array_reference_sources);
        self.dynamic_define |= other.dynamic_define;
        self.ref_capture_writes.extend(other.ref_capture_writes);
        self.exposes_locals |= other.exposes_locals;
        self.place_constructions.extend(other.place_constructions);
    }

    /// Returns true when a PHP function is referenced case-insensitively.
    pub(crate) fn references(&self, name: &str) -> bool {
        self.functions.contains(&php_symbol_key(name))
    }

    /// Returns true when a class is constructed by a statically-known name.
    pub(crate) fn constructs(&self, name: &str) -> bool {
        self.classes.contains(&php_symbol_key(name))
    }
}

/// Collects direct and literal-indirect function references from a program.
pub(crate) fn collect(program: &Program) -> Usage {
    let mut usage = Usage::default();
    for stmt in program {
        scan_stmt(stmt, &mut usage);
    }
    usage
}

/// Collects references from one statement and all nested AST children.
pub(crate) fn collect_stmt(stmt: &Stmt) -> Usage {
    let mut usage = Usage::default();
    scan_stmt(stmt, &mut usage);
    usage
}

/// Returns the variables a closure in this statement list captures by reference and writes.
///
/// A by-reference capture's cell takes ONE representation for the whole body, and a closure that
/// writes through it widens the name to `Mixed`. A reader captured BEFORE the writer was lowered
/// with the old type and read the box as a string, so the decision is made for the body up front.
pub(crate) fn ref_capture_writes(statements: &[Stmt]) -> HashSet<String> {
    let mut usage = Usage::default();
    scan_program(statements, &mut usage);
    usage.ref_capture_writes
}

/// Returns whether a body gives another callee a way to rebind one of its locals; see
/// [`Usage::exposes_locals`].
pub(crate) fn body_exposes_locals(statements: &[Stmt]) -> bool {
    let mut usage = Usage::default();
    scan_program(statements, &mut usage);
    usage.exposes_locals
}

/// Returns the variables a statement list stores into arrays as references.
pub(crate) fn array_reference_sources(statements: &[Stmt]) -> HashSet<String> {
    let mut usage = Usage::default();
    scan_program(statements, &mut usage);
    usage.array_reference_sources
}

/// Records a variable name when it is a `$GLOBALS['…']` alias.
///
/// Called for every position that names a variable with a bare `String` rather
/// than an `ExprKind::Variable` node (assignment targets, array-write receivers,
/// `foreach` binders), so an alias is collected no matter which syntactic form
/// the parser folded it into.
fn record_variable_name(usage: &mut Usage, name: &str) {
    if let Some(target) = crate::globals_array::alias_target(name) {
        usage.globals_keys.insert(target.to_string());
    }
}

/// Records one normalized PHP function name.
fn record_name(usage: &mut Usage, name: &str) {
    usage
        .functions
        .insert(php_symbol_key(name.trim_start_matches('\\')));
}

/// Records one normalized PHP class name used by `new ClassName(...)`.
fn record_class_name(usage: &mut Usage, name: &str) {
    usage
        .classes
        .insert(php_symbol_key(name.trim_start_matches('\\')));
}

/// Records the place arguments of one statically-named construction.
fn record_place_construction(usage: &mut Usage, class_name: &str, args: &[Expr]) {
    let places: Vec<PlaceArgument> = args
        .iter()
        .enumerate()
        .filter_map(|(position, arg)| {
            let (value, name) = match &arg.kind {
                ExprKind::NamedArg { name, value } => (value.as_ref(), Some(name.clone())),
                _ => (arg, None),
            };
            // A declared property, or an element rooted at one, is bound natively: the callee gets
            // the property's reference cell (`Checker::promote_by_ref_property_args`). Symfony's
            // `OrderedHashMap::getIterator()` -- `new OrderedHashMapIterator($this->elements,
            // $this->orderedKeys, $this->managedCursors)` -- runs compiled, php's output and all.
            let mut root = value;
            while let ExprKind::ArrayAccess { array, .. } = &root.kind {
                root = array.as_ref();
            }
            if matches!(root.kind, ExprKind::PropertyAccess { .. }) {
                return None;
            }
            matches!(
                value.kind,
                ExprKind::PropertyAccess { .. }
                    | ExprKind::DynamicPropertyAccess { .. }
                    | ExprKind::NullsafePropertyAccess { .. }
                    | ExprKind::NullsafeDynamicPropertyAccess { .. }
                    | ExprKind::StaticPropertyAccess { .. }
                    | ExprKind::DynamicStaticPropertyAccess { .. }
                    | ExprKind::ArrayAccess { .. }
            )
            .then_some(PlaceArgument { position, name })
        })
        .collect();
    if !places.is_empty() {
        usage.place_constructions.push(PlaceConstruction {
            class_key: php_symbol_key(class_name.trim_start_matches('\\')),
            places,
        });
    }
}

/// Records whether a static receiver requires the runtime called-class identity.
fn record_static_receiver(usage: &mut Usage, receiver: &StaticReceiver) {
    usage.uses_late_static |= matches!(receiver, StaticReceiver::Static);
}

/// Records literal string callbacks passed to a builtin's declared callback parameters.
///
/// The positions come from the shared contract catalog rather than a list repeated here, so a
/// builtin that gains a callback parameter is covered the day its contract says so.
fn record_literal_builtin_callback_arguments(usage: &mut Usage, name: &str, args: &[Expr]) {
    let Some(contract) = elephc_builtin_contract::lookup(name) else {
        return;
    };
    let callbacks = contract.callback_parameter_names();
    if callbacks.is_empty() {
        return;
    }
    for (position, parameter) in contract.params.iter().enumerate() {
        if !callbacks.contains(&parameter.name) {
            continue;
        }
        record_literal_function(usage, args.get(position));
    }
}

/// Returns whether a `define()` call names its constant with a string literal.
fn define_name_is_literal(args: &[Expr]) -> bool {
    let name = args.iter().find_map(|arg| match &arg.kind {
        ExprKind::NamedArg { name, value } if name == "constant_name" => Some(value.as_ref()),
        ExprKind::NamedArg { .. } => None,
        _ => Some(arg),
    });
    matches!(name.map(|arg| &arg.kind), Some(ExprKind::StringLiteral(_)) | None)
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
    for (_, _, default, _) in params {
        if let Some(default) = default {
            scan_expr(default, usage);
        }
    }
}

/// Scans one class property initializer.
fn scan_property(property: &ClassProperty, usage: &mut Usage) {
    if let Some(default) = &property.default {
        scan_expr(default, usage);
    }
}

/// Scans one class-like method body and parameter defaults.
fn scan_method(method: &ClassMethod, usage: &mut Usage) {
    scan_params(&method.params, usage);
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
        StmtKind::Assign { name, value }
        | StmtKind::TypedAssign { name, value, .. } => {
            record_variable_name(usage, name);
            scan_expr(value, usage);
        }
        StmtKind::ArrayPush { array, value } => {
            record_variable_name(usage, array);
            scan_expr(value, usage);
        }
        StmtKind::ListUnpack { vars, value } => {
            for var in vars {
                record_variable_name(usage, var);
            }
            scan_expr(value, usage);
        }
        StmtKind::Echo(expr)
        | StmtKind::Throw(expr)
        | StmtKind::ExprStmt(expr)
        | StmtKind::ConstDecl { value: expr, .. }
        | StmtKind::StaticVar { init: expr, .. }
        | StmtKind::Return(Some(expr)) => scan_expr(expr, usage),
        StmtKind::StaticPropertyAssign {
            receiver, value, ..
        }
        | StmtKind::StaticPropertyArrayPush {
            receiver, value, ..
        } => {
            record_static_receiver(usage, receiver);
            scan_expr(value, usage);
        }
        StmtKind::Include { path, .. } => {
            usage.dynamic_code_load = true;
            scan_expr(path, usage);
        }
        StmtKind::Global { .. } => {
            usage.exposes_locals = true;
        }
        StmtKind::RefAssign { target, source, .. } => {
            usage.exposes_locals = true;
            record_variable_name(usage, target);
            scan_expr(source, usage);
        }
        StmtKind::PropertyRefAssign { object, source: value, .. } => {
            usage.exposes_locals = true;
            scan_expr(object, usage);
            scan_expr(value, usage);
        }
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
        StmtKind::ArrayAssign { array, index, value } => {
            record_variable_name(usage, array);
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            index,
            value,
            ..
        } => {
            record_static_receiver(usage, receiver);
            scan_expr(index, usage);
            scan_expr(value, usage);
        }
        StmtKind::StaticPropertyElementRefAssign {
            receiver,
            index,
            source,
            ..
        } => {
            record_static_receiver(usage, receiver);
            scan_expr(index, usage);
            scan_expr(source, usage);
        }
        StmtKind::DynamicStaticPropertyWrite {
            receiver,
            property,
            index,
            value,
            ..
        } => {
            record_static_receiver(usage, receiver);
            scan_expr(property, usage);
            if let Some(index) = index {
                scan_expr(index, usage);
            }
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
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            value_by_ref,
            body,
        } => {
            usage.exposes_locals |= *value_by_ref;
            scan_expr(array, usage);
            if let Some(key_var) = key_var {
                record_variable_name(usage, key_var);
            }
            record_variable_name(usage, value_var);
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
        StmtKind::FunctionDecl { params, body, .. } => {
            scan_params(params, usage);
            scan_program(body, usage);
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            scan_program(try_body, usage);
            for catch in catches {
                scan_program(&catch.body, usage);
            }
            if let Some(body) = finally_body {
                scan_program(body, usage);
            }
        }
        StmtKind::ClassDecl {
            properties,
            methods,
            constants,
            ..
        }
        | StmtKind::TraitDecl {
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
        StmtKind::Return(None)
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ClassLikeActivate { .. }
        | StmtKind::ExternGlobalDecl { .. } => {}
    }
}

/// Scans every child and callable position of one expression.
fn scan_expr(expr: &Expr, usage: &mut Usage) {
    match &expr.kind {
        // `['Closure', 'fromCallable']` called at run time resolves to the injected helper, which
        // nothing else names: the string is its only reference.
        ExprKind::StringLiteral(text) if crate::closure_from_callable_prelude::names_it(text) => {
            record_class_name(usage, crate::closure_from_callable_prelude::CLOSURE_FACTORY_CLASS);
        }
        ExprKind::IncludeValue { path, .. } => {
            usage.dynamic_code_load = true;
            usage.value_include = true;
            scan_expr(path, usage);
        }
        ExprKind::FunctionCall { name, args } => {
            // An unqualified call inside a namespace resolves to its GLOBAL name at run time when
            // the namespace has no such function. Recording only the namespaced spelling hid it
            // from every prelude that is injected on use: symfony/routing's
            // `AttributeFileLoader::findClass()` calls `token_get_all()` from
            // `Symfony\Component\Routing\Loader`, the tokenizer prelude was never built, and the
            // call fell through to an interpreter that has no tokenizer.
            if let Some(global) = name.function_fallback() {
                record_name(usage, global);
            }
            let name = php_symbol_key(name.as_str().trim_start_matches('\\'));
            record_name(usage, &name);
            if name.rsplit('\\').next() == Some("define") && !define_name_is_literal(args) {
                usage.dynamic_define = true;
            }
            // Only the multi-array form of `array_map()` is lowered to its prelude helper, so
            // only that form pulls the helper (and its callable invokers) into the program.
            let bare = name.rsplit('\\').next().unwrap_or(&name);
            // Every `exec()` call is lowered to
            // its prelude helper, which strips trailing whitespace and fills the by-reference outputs.
            if bare == "exec" {
                record_name(usage, crate::exec_prelude::EXEC_FULL_NAME);
            }
            // The `&$result_code` form of `passthru()` is lowered to the same prelude family.
            if bare == "passthru" && args.len() == 2 {
                record_name(usage, crate::exec_prelude::EXEC_FULL_NAME);
            }
            // The long forms of `fputcsv()` (`$escape`, `$eol`) and `fgetcsv()` (`$enclosure`,
            // `$escape`) are lowered to their prelude helpers; the native runtimes know only the
            // default dialect.
            if bare == "fputcsv" {
                record_name(usage, crate::csv_prelude::FPUTCSV_FULL_NAME);
            }
            // So are the length / context forms of `fgets()`, `fwrite()`, `rmdir()` and friends.
            if let Some(helper) = crate::stream_args_prelude::helper_for(bare, args.len()) {
                record_name(usage, helper);
            }
            if bare == "fgetcsv" && args.len() >= 4 {
                record_name(usage, crate::csv_prelude::FGETCSV_FULL_NAME);
            }
            // Every `stream_select()` call is lowered to its prelude helper, which accepts null,
            // hash and boxed arrays and keeps the ready entries' keys.
            if bare == "stream_select" {
                record_name(usage, crate::stream_select_prelude::STREAM_SELECT_NAME);
            }
            // A two-argument `count()` / `sizeof()` (an explicit `$mode`) may be routed to its
            // nested-array helper; the decision needs the receiver's type.
            if (bare == "count" || bare == "sizeof") && args.len() == 2 {
                record_name(usage, crate::count_recursive_prelude::COUNT_RECURSIVE_NAME);
            }
            // A one-argument `min()` / `max()` over a gradual operand, and a multi-argument one with
            // a string operand, may be routed to its helper; the decision needs the operand types,
            // so the helper is declared for every such call.
            if bare == "min" || bare == "max" {
                record_name(usage, crate::min_max_prelude::MIN_MAX_MIXED_NAME);
            }
            // A math builtin's `string` first argument may be routed through its conversion
            // helper; the decision needs the operand's type.
            if crate::numeric_string_arg_prelude::NUMERIC_ARG_BUILTINS.contains(&bare)
                && !args.is_empty()
            {
                record_name(
                    usage,
                    crate::numeric_string_arg_prelude::NUMERIC_STRING_ARG_NAME,
                );
            }
            // `array_fill_keys()` over gradual keys may be routed to its helper; the decision needs
            // the keys' type.
            if bare == "array_fill_keys" {
                record_name(usage, crate::array_fill_keys_prelude::ARRAY_FILL_KEYS_MIXED_NAME);
            }
            if bare == "array_map" && args.len() > 2 {
                record_name(usage, crate::array_map_prelude::ARRAY_MAP_MULTI_NAME);
            }
            // A single-array map whose callback is read from storage (a property, an element) may
            // be selected at run time; `compat_preludes` routes it to the gradual helper.
            if bare == "array_map"
                && args.len() == 2
                && matches!(
                    args[0].kind,
                    ExprKind::PropertyAccess { .. }
                        | ExprKind::NullsafePropertyAccess { .. }
                        | ExprKind::StaticPropertyAccess { .. }
                        | ExprKind::ArrayAccess { .. }
                )
            {
                record_name(usage, crate::array_map_prelude::ARRAY_MAP_GRADUAL_NAME);
            }
            // Only the `$syntax_only` / `$callable_name` forms are lowered to the extended helper
            // (`compat_preludes::lower_is_callable_with_options`); the one-argument form stays on
            // the backend predicate. Naming the helper for every `is_callable()` pulled it, and
            // the dynamic-callable hazard in its body, into any program that merely
            // contained one -- PDO's own prelude calls it -- so the forced backend-gap group kept
            // every declaration alive.
            if bare == "is_callable" && matches!(args.len(), 2 | 3) {
                record_name(usage, crate::is_callable_prelude::IS_CALLABLE_EXT_NAME);
            }
            if bare == "array_keys" && args.len() > 1 {
                record_name(usage, crate::array_keys_prelude::ARRAY_KEYS_SEARCH_NAME);
            }
            // The array builtins whose operands or flags php settles at run time lower to
            // `crate::array_gradual_prelude`'s helpers, for these call shapes only.
            crate::array_gradual_prelude::record_call_shape(
                &mut |helper| record_name(usage, helper),
                bare,
                args,
            );
            match name.as_str() {
                "function_exists" | "is_callable" => {
                    if matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::StringLiteral(_))) {
                        record_literal_function(usage, args.first());
                    } else {
                        usage.dynamic_function_call = true;
                    }
                }
                "call_user_func" | "call_user_func_array" => {
                    if matches!(args.first().map(|arg| &arg.kind), Some(ExprKind::StringLiteral(_))) {
                        record_literal_function(usage, args.first());
                    } else {
                        usage.dynamic_function_call = true;
                    }
                }
                "eval" => usage.dynamic_function_call = true,
                // Every OTHER builtin that takes a callback names its callback parameters in
                // the shared contract catalog, so a literal callback reaches the same
                // "this function is referenced" record as `call_user_func('f')` does. Without
                // it `uksort($paths, 'strnatcmp')` was not a reference to `strnatcmp` at all:
                // the backend-gap prelude that implements it was never injected, and the
                // native sort then jumped through an unresolved callable.
                _ => record_literal_builtin_callback_arguments(usage, &name, args),
            }
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
            record_name(usage, name.as_str());
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            scan_expr(object, usage);
        }
        ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, .. }) => {
            record_static_receiver(usage, receiver);
        }
        ExprKind::ExprCall { callee, args } => {
            usage.dynamic_function_call = true;
            scan_expr(callee, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::ClosureCall { var, args } => {
            usage.dynamic_function_call = true;
            record_variable_name(usage, var);
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
            if let InstanceOfTarget::Expr(target) = target {
                scan_expr(target, usage);
            }
        }
        ExprKind::ArrayReference(inner) => {
            usage.exposes_locals = true;
            if let ExprKind::Variable(name) = &inner.kind {
                usage.array_reference_sources.insert(name.clone());
            }
            scan_expr(inner, usage);
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
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::YieldFrom(inner) => scan_expr(inner, usage),
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
            for entry in entries {
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
        ExprKind::Closure { params, body, capture_refs, .. } => {
            usage.exposes_locals |= !capture_refs.is_empty();
            for name in capture_refs {
                if crate::ir_lower::body_writes_local(body, name) {
                    usage.ref_capture_writes.insert(name.clone());
                }
            }
            scan_params(params, usage);
            scan_program(body, usage);
        }
        ExprKind::NamedArg { value, .. } => scan_expr(value, usage),
        ExprKind::NewObject { class_name, args } => {
            record_class_name(usage, class_name.as_str());
            record_place_construction(usage, class_name.as_str(), args);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewGeneric { class_type, args } => {
            // Pre-instantiation form of `new Box<User>(...)`: the head and every type argument
            // are class positions, exactly like a plain `new`.
            for name in class_type.named_classes() {
                record_class_name(usage, name.as_str());
            }
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::StaticMethodCall { receiver, args, .. }
        | ExprKind::NewScopedObject { receiver, args } => {
            record_static_receiver(usage, receiver);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            scan_expr(name_expr, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
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
        ExprKind::DynamicStaticPropertyAccess {
            receiver, property, ..
        } => {
            record_static_receiver(usage, receiver);
            scan_expr(property, usage);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
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
            scan_expr(object, usage);
            scan_expr(method, usage);
            for arg in args {
                scan_expr(arg, usage);
            }
        }
        ExprKind::BufferNew { len, .. } => scan_expr(len, usage),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                scan_expr(key, usage);
            }
            if let Some(value) = value {
                scan_expr(value, usage);
            }
        }
        ExprKind::Variable(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => record_variable_name(usage, name),
        // A variable holding the name (`$f = 'array_merge'; $f($a, $b)`) resolves to the builtin
        // during lowering, which then routes keyed operands to this prelude's helpers; only the
        // literal marks the program as needing them.
        ExprKind::StringLiteral(value) => {
            if value.eq_ignore_ascii_case("array_merge") {
                record_name(usage, value);
            }
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::MagicConstant(_) => {}
        ExprKind::StaticPropertyAccess { receiver, .. }
        | ExprKind::ClassConstant { receiver }
        | ExprKind::ScopedConstantAccess { receiver, .. } => {
            record_static_receiver(usage, receiver);
        }
        ExprKind::ObjectClassName { object } => scan_expr(object, usage),
        ExprKind::DynamicScopedConstantAccess { receiver, .. } => scan_expr(receiver, usage),
    }
}
