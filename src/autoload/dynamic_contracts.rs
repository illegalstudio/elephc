//! Purpose:
//! Resolves bounded, dynamically constructed class strings for classes the
//! program already constructs through ordinary static syntax.
//!
//! Called from:
//! - `crate::autoload::run_collecting_included_with_defines_and_sources()`.
//!
//! Key details:
//! - Shapes considered: a `class_exists($name = $this->method()) ? new $name() : ...` factory,
//!   class strings derived from `static::class` (assigned or returned), and concatenations of
//!   literals whose ternary branches spell a bounded set of names.
//! - The partial evaluator is deliberately conservative: unsupported expressions produce no
//!   candidate instead of guessing or importing optional declarations from the autoload index.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::parser::ast::{BinOp, ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind};

/// Returns class names computed by guarded dynamic factories reachable from direct construction.
pub(super) fn candidates(program: &Program) -> Vec<String> {
    let classes = ClassIndex::from_program(program);
    let mut roots = HashSet::default();
    collect_constructed_classes(program, &mut roots);

    let mut candidates = HashSet::default();
    for root in roots {
        for method in guarded_factory_targets(&classes, &root) {
            if let Some(Value::String(name)) = evaluate_method(&classes, &root, &method, &mut HashSet::default()) {
                if !name.is_empty() {
                    candidates.insert(name);
                }
            }
        }
        candidates.extend(property_mapped_dynamic_new_candidates(&classes, &root));
    }
    candidates.extend(late_static_class_string_candidates(&classes));
    candidates.extend(concatenated_class_string_candidates(program));

    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    candidates.sort();
    candidates
}

/// The most strings one concatenation may spell before it stops being a candidate source.
const MAX_CONCATENATED_SPELLINGS: usize = 16;

/// Returns the class names a concatenation of literals can spell, each branch of a ternary
/// included: `'Acme\\Lib\\' . ($argc > 5 ? 'Other' : 'Listed')` names both classes, and
/// `new $name` reaches either one through the autoloader. Only the names the autoload index
/// resolves are loaded, so a string that names no class costs nothing.
fn concatenated_class_string_candidates(program: &Program) -> HashSet<String> {
    use crate::parser::ast::visit::{walk_nodes, Node};
    let mut candidates = HashSet::default();
    // The operands of a visited concatenation: only the whole one spells a name, its prefix
    // `'Acme\\Rule\\' . $branch` inside `... . 'Rule'` does not.
    let mut inner: HashSet<*const Expr> = HashSet::default();
    walk_nodes(program, &mut |node| {
        let Node::Expr(expr) = node else { return };
        let ExprKind::BinaryOp { left, op: BinOp::Concat, right } = &expr.kind else {
            return;
        };
        inner.insert(&**left as *const Expr);
        inner.insert(&**right as *const Expr);
        if inner.contains(&(expr as *const Expr)) {
            return;
        }
        for spelling in literal_spellings(expr).unwrap_or_default() {
            if spelling.contains('\\') && !spelling.contains(char::is_whitespace) {
                candidates.insert(spelling.trim_start_matches('\\').to_string());
            }
        }
    });
    candidates
}

/// Every string `expr` can evaluate to when it is built only from string literals, concatenation
/// and ternary branches; `None` for anything else or past `MAX_CONCATENATED_SPELLINGS`.
fn literal_spellings(expr: &Expr) -> Option<Vec<String>> {
    let spellings = match &expr.kind {
        ExprKind::StringLiteral(value) => vec![value.clone()],
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let mut spellings = literal_spellings(then_expr)?;
            spellings.extend(literal_spellings(else_expr)?);
            spellings
        }
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let left = literal_spellings(left)?;
            let right = literal_spellings(right)?;
            if left.len() * right.len() > MAX_CONCATENATED_SPELLINGS {
                return None;
            }
            left.iter()
                .flat_map(|left| right.iter().map(move |right| format!("{left}{right}")))
                .collect()
        }
        _ => return None,
    };
    (spellings.len() <= MAX_CONCATENATED_SPELLINGS).then_some(spellings)
}

/// Returns the class strings an inherited method derives from `static::class` for each concrete
/// class of the program.
///
/// symfony/dependency-injection's `Extension::getConfiguration()` does
/// `$class = substr_replace(static::class, '\\Configuration', strrpos(static::class, '\\'));` and
/// hands the result to the container's reflection: every bundle extension finds its
/// `Configuration` class by that name, and nothing else in the program names TwigBundle's. Each
/// assignment whose value mentions `static::class` is evaluated once per concrete class that
/// inherits it, with `static::class` bound to that class; a value that is not a class name the
/// autoload index can resolve costs nothing, since only resolvable names are ever loaded.
fn late_static_class_string_candidates(classes: &ClassIndex<'_>) -> HashSet<String> {
    let mut candidates = HashSet::default();
    for class in classes.classes.values().filter(|class| !class.is_abstract) {
        let mut current = Some(class.name.clone());
        let mut seen = HashSet::default();
        while let Some(owner_name) = current {
            let key = crate::names::php_symbol_key(&owner_name);
            if !seen.insert(key.clone()) {
                break;
            }
            let Some(owner) = classes.classes.get(&key) else {
                break;
            };
            for method in owner.methods {
                // In source order, so `$class = static::class; $class = substr_replace($class, ..)`
                // sees the first value in the second. A local is "derived" once its value came
                // from `static::class` or from another derived local; only derived values are
                // candidates, so ordinary string locals never widen the world.
                let mut locals: HashMap<String, Value> = HashMap::default();
                let mut derived: HashSet<String> = HashSet::default();
                crate::parser::ast::visit::walk_stmts(&method.body, &mut |stmt| {
                    // A returned class string counts too: symfony/validator's
                    // `Constraint::validatedBy()` is `return static::class.'Validator';`, and the
                    // constraint validator factory builds `new $className()` from it.
                    if let StmtKind::Return(Some(value)) = &stmt.kind {
                        if mentions_static_class(value) || reads_any_local(value, &derived) {
                            if let Some(Value::String(candidate)) = evaluate_expr(
                                classes,
                                &class.name,
                                &owner.name,
                                value,
                                &mut locals,
                                &mut HashSet::default(),
                            ) {
                                if candidate.contains('\\') && !candidate.contains(char::is_whitespace) {
                                    candidates.insert(candidate.trim_start_matches('\\').to_string());
                                }
                            }
                        }
                        return;
                    }
                    let StmtKind::Assign { name, value } = &stmt.kind else {
                        return;
                    };
                    let is_derived = mentions_static_class(value)
                        || reads_any_local(value, &derived);
                    let result = evaluate_expr(
                        classes,
                        &class.name,
                        &owner.name,
                        value,
                        &mut locals,
                        &mut HashSet::default(),
                    );
                    match result {
                        Some(result) => {
                            if is_derived {
                                derived.insert(name.clone());
                                if let Value::String(candidate) = &result {
                                    if candidate.contains('\\') && !candidate.contains(char::is_whitespace) {
                                        candidates.insert(candidate.trim_start_matches('\\').to_string());
                                    }
                                }
                            } else {
                                derived.remove(name);
                            }
                            locals.insert(name.clone(), result);
                        }
                        None => {
                            locals.remove(name);
                            derived.remove(name);
                        }
                    }
                });
            }
            current = owner.parent.clone();
        }
    }
    candidates
}

/// Whether `expr` reads one of `locals`.
fn reads_any_local(expr: &Expr, locals: &HashSet<String>) -> bool {
    if locals.is_empty() {
        return false;
    }
    let mut found = false;
    crate::parser::ast::visit::walk_nodes(
        std::slice::from_ref(&Stmt::new(StmtKind::ExprStmt(expr.clone()), expr.span)),
        &mut |node| {
            if let crate::parser::ast::visit::Node::Expr(inner) = node {
                if matches!(&inner.kind, ExprKind::Variable(name) if locals.contains(name)) {
                    found = true;
                }
            }
        },
    );
    found
}

/// Whether `expr` reads `static::class` anywhere.
fn mentions_static_class(expr: &Expr) -> bool {
    let mut found = false;
    crate::parser::ast::visit::walk_nodes(
        std::slice::from_ref(&Stmt::new(StmtKind::ExprStmt(expr.clone()), expr.span)),
        &mut |node| {
            if let crate::parser::ast::visit::Node::Expr(inner) = node {
                if matches!(
                    inner.kind,
                    ExprKind::ClassConstant { receiver: StaticReceiver::Static }
                ) {
                    found = true;
                }
            }
        },
    );
    found
}

/// Returns literal class strings stored in a constructed class's property map when it dynamically constructs values.
///
/// A class-string map is a common PHP factory shape: its constructor stores `ClassName::class`
/// values in an instance property, then another method selects an entry and executes `new $name`.
/// The compiler cannot execute the selector, but every literal candidate is bounded by the already
/// constructed class and can enter ordinary autoload resolution before lowering the dynamic call.
fn property_mapped_dynamic_new_candidates(classes: &ClassIndex<'_>, root: &str) -> HashSet<String> {
    let mut candidates = HashSet::default();
    let mut current = root.to_string();
    let mut seen = HashSet::default();
    loop {
        let key = crate::names::php_symbol_key(&current);
        if !seen.insert(key.clone()) {
            break;
        }
        let Some(class) = classes.classes.get(&key) else {
            break;
        };
        if class
            .methods
            .iter()
            .any(|method| statements_contain_dynamic_new(&method.body))
        {
            for constructor in class
                .methods
                .iter()
                .filter(|method| crate::names::php_symbol_key(&method.name) == "__construct")
            {
                collect_constructor_property_class_strings(&constructor.body, &mut candidates);
            }
        }
        let Some(parent) = &class.parent else {
            break;
        };
        current = parent.clone();
    }
    candidates
}

/// Returns whether a statement tree contains a plain PHP `new $class` construction.
fn statements_contain_dynamic_new(body: &[Stmt]) -> bool {
    body.iter().any(statement_contains_dynamic_new)
}

/// Returns whether one statement or nested control-flow body contains a dynamic construction.
fn statement_contains_dynamic_new(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ExprStmt(value)
        | StmtKind::Return(Some(value))
        | StmtKind::Throw(value) => expression_contains_dynamic_new(value),
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            expression_contains_dynamic_new(condition)
                || statements_contain_dynamic_new(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(condition, body)| {
                        expression_contains_dynamic_new(condition)
                            || statements_contain_dynamic_new(body)
                    })
                || else_body
                    .as_ref()
                    .is_some_and(|body| statements_contain_dynamic_new(body))
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            expression_contains_dynamic_new(condition) || statements_contain_dynamic_new(body)
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            init.as_deref().is_some_and(statement_contains_dynamic_new)
                || condition
                    .as_ref()
                    .is_some_and(expression_contains_dynamic_new)
                || update.as_deref().is_some_and(statement_contains_dynamic_new)
                || statements_contain_dynamic_new(body)
        }
        StmtKind::Foreach { array, body, .. } => {
            expression_contains_dynamic_new(array) || statements_contain_dynamic_new(body)
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            statements_contain_dynamic_new(try_body)
                || catches
                    .iter()
                    .any(|catch| statements_contain_dynamic_new(&catch.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| statements_contain_dynamic_new(body))
        }
        _ => false,
    }
}

/// Returns whether an expression tree contains a plain PHP `new $class` construction.
fn expression_contains_dynamic_new(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::NewDynamic { .. } => true,
        ExprKind::Assignment { value, prelude, .. } => {
            expression_contains_dynamic_new(value) || statements_contain_dynamic_new(prelude)
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::ShortTernary {
            value: left,
            default: right,
        }
        | ExprKind::ArrayAccess {
            array: left,
            index: right,
        } => expression_contains_dynamic_new(left) || expression_contains_dynamic_new(right),
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expression_contains_dynamic_new(condition)
                || expression_contains_dynamic_new(then_expr)
                || expression_contains_dynamic_new(else_expr)
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => args.iter().any(expression_contains_dynamic_new),
        ExprKind::ExprCall { callee, args } => {
            expression_contains_dynamic_new(callee) || args.iter().any(expression_contains_dynamic_new)
        }
        ExprKind::MethodCall { object, args, .. } | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expression_contains_dynamic_new(object) || args.iter().any(expression_contains_dynamic_new)
        }
        ExprKind::ArrayLiteral(values) => values.iter().any(expression_contains_dynamic_new),
        ExprKind::ArrayLiteralAssoc(values) => values.iter().any(|(key, value)| {
            expression_contains_dynamic_new(key) || expression_contains_dynamic_new(value)
        }),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::ObjectClassName { object } => expression_contains_dynamic_new(object),
        _ => false,
    }
}

/// Collects literal `ClassName::class` values assigned to instance properties during construction.
fn collect_constructor_property_class_strings(body: &[Stmt], candidates: &mut HashSet<String>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::PropertyAssign { object, value, .. } if matches!(object.kind, ExprKind::This) => {
                collect_literal_class_strings(value, candidates);
            }
            StmtKind::ExprStmt(expr) => {
                collect_property_assignment_class_strings(expr, candidates);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_constructor_property_class_strings(then_body, candidates);
                for (_, body) in elseif_clauses {
                    collect_constructor_property_class_strings(body, candidates);
                }
                if let Some(body) = else_body {
                    collect_constructor_property_class_strings(body, candidates);
                }
            }
            _ => {}
        }
    }
}

/// Collects class strings from expression-form assignments such as `$this->map = [Type::class]`.
fn collect_property_assignment_class_strings(expr: &Expr, candidates: &mut HashSet<String>) {
    let ExprKind::Assignment { target, value, .. } = &expr.kind else {
        return;
    };
    if matches!(&target.kind, ExprKind::PropertyAccess { object, .. } if matches!(&object.kind, ExprKind::This)) {
        collect_literal_class_strings(value, candidates);
    }
}

/// Collects literal class-string constants nested in an array literal or transparent expression.
fn collect_literal_class_strings(expr: &Expr, candidates: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => {
            candidates.insert(name.as_canonical().trim_start_matches('\\').to_string());
        }
        ExprKind::ArrayLiteral(values) => {
            for value in values {
                collect_literal_class_strings(value, candidates);
            }
        }
        ExprKind::ArrayLiteralAssoc(values) => {
            for (_, value) in values {
                collect_literal_class_strings(value, candidates);
            }
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            collect_literal_class_strings(then_expr, candidates);
            collect_literal_class_strings(else_expr, candidates);
        }
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_literal_class_strings(value, candidates);
            collect_literal_class_strings(default, candidates);
        }
        _ => {}
    }
}

/// Indexed class metadata needed to resolve a no-argument instance method on an inheritance path.
struct ClassInfo<'a> {
    name: String,
    parent: Option<String>,
    methods: &'a [ClassMethod],
    is_abstract: bool,
}

/// Class declarations currently in the expanded program, keyed by PHP's case-insensitive symbol key.
struct ClassIndex<'a> {
    classes: HashMap<String, ClassInfo<'a>>,
}

impl<'a> ClassIndex<'a> {
    /// Builds a class lookup table from the declarations that have already entered the program.
    fn from_program(program: &'a Program) -> Self {
        let mut classes = HashMap::default();
        collect_class_declarations(program, &mut classes);
        Self { classes }
    }

    /// Finds an instance method on `class_name` or one of its declared parents.
    fn method(&self, class_name: &str, method_name: &str) -> Option<(&ClassInfo<'a>, &'a ClassMethod)> {
        let mut current = class_name.to_string();
        let method_key = crate::names::php_symbol_key(method_name);
        let mut seen = HashSet::default();
        loop {
            let key = crate::names::php_symbol_key(&current);
            if !seen.insert(key.clone()) {
                return None;
            }
            let class = self.classes.get(&key)?;
            if let Some(method) = class
                .methods
                .iter()
                .find(|method| !method.is_static && crate::names::php_symbol_key(&method.name) == method_key)
            {
                return Some((class, method));
            }
            current = class.parent.clone()?;
        }
    }
}

/// Adds class declarations, including declarations nested in transparent program wrappers.
fn collect_class_declarations<'a>(body: &'a [Stmt], classes: &mut HashMap<String, ClassInfo<'a>>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::ClassDecl {
                name,
                extends,
                methods,
                is_abstract,
                ..
            } => {
                let name = name.trim_start_matches('\\').to_string();
                classes.insert(
                    crate::names::php_symbol_key(&name),
                    ClassInfo {
                        name,
                        parent: extends
                            .as_ref()
                            .map(|parent| parent.as_canonical().trim_start_matches('\\').to_string()),
                        methods,
                        is_abstract: *is_abstract,
                    },
                );
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => collect_class_declarations(body, classes),
            _ => {}
        }
    }
}

/// Collects statically named classes that program code can construct at runtime.
fn collect_constructed_classes(body: &[Stmt], out: &mut HashSet<String>) {
    for stmt in body {
        collect_constructed_classes_from_stmt(stmt, out);
    }
}

/// Visits one statement for direct construction expressions.
fn collect_constructed_classes_from_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Assign { value, .. }
        | StmtKind::TypedAssign { value, .. }
        | StmtKind::ArrayPush { value, .. }
        | StmtKind::Echo(value)
        | StmtKind::Throw(value)
        | StmtKind::ExprStmt(value)
        | StmtKind::ConstDecl { value, .. }
        | StmtKind::StaticVar { init: value, .. }
        | StmtKind::Return(Some(value)) => collect_constructed_classes_from_expr(value, out),
        StmtKind::ArrayAssign { index, value, .. }
        | StmtKind::NestedArrayAssign { target: index, value } => {
            collect_constructed_classes_from_expr(index, out);
            collect_constructed_classes_from_expr(value, out);
        }
        StmtKind::PropertyAssign { object, value, .. }
        | StmtKind::PropertyRefAssign {
            object,
            source: value,
            ..
        }
        | StmtKind::PropertyArrayPush { object, value, .. } => {
            collect_constructed_classes_from_expr(object, out);
            collect_constructed_classes_from_expr(value, out);
        }
        StmtKind::StaticPropertyAssign { value, .. }
        | StmtKind::StaticPropertyArrayPush { value, .. } => collect_constructed_classes_from_expr(value, out),
        StmtKind::PropertyArrayAssign {
            object,
            index,
            value,
            ..
        } => {
            collect_constructed_classes_from_expr(object, out);
            collect_constructed_classes_from_expr(index, out);
            collect_constructed_classes_from_expr(value, out);
        }
        StmtKind::StaticPropertyArrayAssign { index, value, .. }
        | StmtKind::StaticPropertyElementRefAssign {
            index,
            source: value,
            ..
        } => {
            collect_constructed_classes_from_expr(index, out);
            collect_constructed_classes_from_expr(value, out);
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            collect_constructed_classes_from_expr(condition, out);
            collect_constructed_classes(then_body, out);
            for (condition, body) in elseif_clauses {
                collect_constructed_classes_from_expr(condition, out);
                collect_constructed_classes(body, out);
            }
            if let Some(body) = else_body {
                collect_constructed_classes(body, out);
            }
        }
        StmtKind::While { condition, body } | StmtKind::DoWhile { condition, body } => {
            collect_constructed_classes_from_expr(condition, out);
            collect_constructed_classes(body, out);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            if let Some(init) = init {
                collect_constructed_classes_from_stmt(init, out);
            }
            if let Some(condition) = condition {
                collect_constructed_classes_from_expr(condition, out);
            }
            if let Some(update) = update {
                collect_constructed_classes_from_stmt(update, out);
            }
            collect_constructed_classes(body, out);
        }
        StmtKind::Foreach { array, body, .. } => {
            collect_constructed_classes_from_expr(array, out);
            collect_constructed_classes(body, out);
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            collect_constructed_classes_from_expr(subject, out);
            for (values, body) in cases {
                for value in values {
                    collect_constructed_classes_from_expr(value, out);
                }
                collect_constructed_classes(body, out);
            }
            if let Some(body) = default {
                collect_constructed_classes(body, out);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_constructed_classes(try_body, out);
            for catch in catches {
                collect_constructed_classes(&catch.body, out);
            }
            if let Some(body) = finally_body {
                collect_constructed_classes(body, out);
            }
        }
        StmtKind::FunctionDecl { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => collect_constructed_classes(body, out),
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. } => {
            for method in methods {
                collect_constructed_classes(&method.body, out);
            }
        }
        _ => {}
    }
}

/// Visits one expression for direct construction expressions.
fn collect_constructed_classes_from_expr(expr: &Expr, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::NewObject { class_name, args } => {
            out.insert(class_name.as_canonical().trim_start_matches('\\').to_string());
            for arg in args {
                collect_constructed_classes_from_expr(arg, out);
            }
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::NullCoalesce {
            value: left,
            default: right,
        }
        | ExprKind::ShortTernary {
            value: left,
            default: right,
        }
        | ExprKind::ArrayAccess {
            array: left,
            index: right,
        } => {
            collect_constructed_classes_from_expr(left, out);
            collect_constructed_classes_from_expr(right, out);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_constructed_classes_from_expr(condition, out);
            collect_constructed_classes_from_expr(then_expr, out);
            collect_constructed_classes_from_expr(else_expr, out);
        }
        ExprKind::Assignment { target, value, prelude, .. } => {
            collect_constructed_classes_from_expr(target, out);
            collect_constructed_classes_from_expr(value, out);
            collect_constructed_classes(prelude, out);
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::Spread(value)
        | ExprKind::Clone(value)
        | ExprKind::YieldFrom(value) => collect_constructed_classes_from_expr(value, out),
        ExprKind::FunctionCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewDynamic { args, .. }
        | ExprKind::NewDynamicObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            for arg in args {
                collect_constructed_classes_from_expr(arg, out);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_constructed_classes_from_expr(callee, out);
            for arg in args {
                collect_constructed_classes_from_expr(arg, out);
            }
        }
        ExprKind::ArrayLiteral(values) => {
            for value in values {
                collect_constructed_classes_from_expr(value, out);
            }
        }
        ExprKind::ArrayLiteralAssoc(values) => {
            for (key, value) in values {
                collect_constructed_classes_from_expr(key, out);
                collect_constructed_classes_from_expr(value, out);
            }
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::DynamicPropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::NullsafeDynamicPropertyAccess { object, .. }
        | ExprKind::ObjectClassName { object }
        | ExprKind::MethodCall { object, .. }
        | ExprKind::NullsafeMethodCall { object, .. }
        | ExprKind::NullsafeDynamicMethodCall { object, .. } => collect_constructed_classes_from_expr(object, out),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_constructed_classes_from_expr(subject, out);
            for (patterns, value) in arms {
                for pattern in patterns {
                    collect_constructed_classes_from_expr(pattern, out);
                }
                collect_constructed_classes_from_expr(value, out);
            }
            if let Some(value) = default {
                collect_constructed_classes_from_expr(value, out);
            }
        }
        ExprKind::Closure { body, .. } => collect_constructed_classes(body, out),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_constructed_classes_from_expr(key, out);
            }
            if let Some(value) = value {
                collect_constructed_classes_from_expr(value, out);
            }
        }
        _ => {}
    }
}

/// Returns no-argument methods supplying class strings to guarded dynamic factories on `root`.
fn guarded_factory_targets(classes: &ClassIndex<'_>, root: &str) -> Vec<String> {
    let mut targets = HashSet::default();
    let mut current = root.to_string();
    let mut seen = HashSet::default();
    loop {
        let key = crate::names::php_symbol_key(&current);
        if !seen.insert(key.clone()) {
            break;
        }
        let Some(class) = classes.classes.get(&key) else {
            break;
        };
        for method in class.methods {
            collect_guarded_factory_targets(&method.body, &mut targets);
        }
        let Some(parent) = &class.parent else {
            break;
        };
        current = parent.clone();
    }
    let mut targets = targets.into_iter().collect::<Vec<_>>();
    targets.sort();
    targets
}

/// Finds method names in `class_exists($name = $this->method()) ? new $name() : ...` returns.
fn collect_guarded_factory_targets(body: &[Stmt], targets: &mut HashSet<String>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => collect_guarded_factory_target(expr, targets),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_guarded_factory_targets(then_body, targets);
                for (_, body) in elseif_clauses {
                    collect_guarded_factory_targets(body, targets);
                }
                if let Some(body) = else_body {
                    collect_guarded_factory_targets(body, targets);
                }
            }
            _ => {}
        }
    }
}

/// Records the receiver method only when the existence probe and dynamic construction share one local.
fn collect_guarded_factory_target(expr: &Expr, targets: &mut HashSet<String>) {
    let ExprKind::Ternary {
        condition,
        then_expr,
        else_expr,
    } = &expr.kind
    else {
        return;
    };
    let Some((local, method)) = class_exists_method_assignment(condition) else {
        return;
    };
    if dynamic_new_uses_local(then_expr, local) || dynamic_new_uses_local(else_expr, local) {
        targets.insert(method.to_string());
    }
}

/// Extracts `$name = $this->method()` from a one-argument `class_exists()` condition.
fn class_exists_method_assignment(expr: &Expr) -> Option<(&str, &str)> {
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if !name
        .as_canonical()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("class_exists")
    {
        return None;
    }
    let ExprKind::Assignment { target, value, .. } = &args.first()?.kind else {
        return None;
    };
    let ExprKind::Variable(local) = &target.kind else {
        return None;
    };
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &value.kind
    else {
        return None;
    };
    if args.is_empty() && matches!(object.kind, ExprKind::This) {
        Some((local, method))
    } else {
        None
    }
}

/// Returns whether a dynamic construction receives the exact local probed by `class_exists()`.
fn dynamic_new_uses_local(expr: &Expr, local: &str) -> bool {
    match &expr.kind {
        ExprKind::NewDynamic { name_expr, .. } => {
            matches!(&name_expr.kind, ExprKind::Variable(name) if name == local)
        }
        ExprKind::NewDynamicObject { class_name, .. } => {
            matches!(&class_name.kind, ExprKind::Variable(name) if name == local)
        }
        _ => false,
    }
}

/// Values supported by the side-effect-free class-string partial evaluator.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    String(String),
    Int(i64),
    Bool(bool),
    Null,
}

/// Evaluates a no-argument instance method under the concrete `root` class's late-static binding.
fn evaluate_method(
    classes: &ClassIndex<'_>,
    root: &str,
    method_name: &str,
    active: &mut HashSet<String>,
) -> Option<Value> {
    let (owner, method) = classes.method(root, method_name)?;
    if !method.params.is_empty() || method.variadic.is_some() {
        return None;
    }
    let key = format!("{}::{method_name}", crate::names::php_symbol_key(root));
    if !active.insert(key.clone()) {
        return None;
    }
    let result = evaluate_statements(classes, root, &owner.name, &method.body, &mut HashMap::default(), active);
    active.remove(&key);
    result
}

/// Evaluates the supported straight-line subset of a method body until its first value return.
fn evaluate_statements(
    classes: &ClassIndex<'_>,
    root: &str,
    owner: &str,
    body: &[Stmt],
    locals: &mut HashMap<String, Value>,
    active: &mut HashSet<String>,
) -> Option<Value> {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Assign { name, value } => {
                let value = evaluate_expr(classes, root, owner, value, locals, active)?;
                locals.insert(name.clone(), value);
            }
            StmtKind::ExprStmt(expr) => {
                evaluate_expr(classes, root, owner, expr, locals, active)?;
            }
            StmtKind::Return(Some(expr)) => {
                return evaluate_expr(classes, root, owner, expr, locals, active);
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                if matches!(evaluate_expr(classes, root, owner, condition, locals, active)?, Value::Bool(true)) {
                    return evaluate_statements(classes, root, owner, then_body, locals, active);
                }
                for (condition, body) in elseif_clauses {
                    if matches!(evaluate_expr(classes, root, owner, condition, locals, active)?, Value::Bool(true)) {
                        return evaluate_statements(classes, root, owner, body, locals, active);
                    }
                }
                if let Some(body) = else_body {
                    return evaluate_statements(classes, root, owner, body, locals, active);
                }
            }
            _ => return None,
        }
    }
    None
}

/// Evaluates a pure expression used to derive a class-string candidate, or returns `None` if unknown.
fn evaluate_expr(
    classes: &ClassIndex<'_>,
    root: &str,
    owner: &str,
    expr: &Expr,
    locals: &mut HashMap<String, Value>,
    active: &mut HashSet<String>,
) -> Option<Value> {
    match &expr.kind {
        ExprKind::StringLiteral(value) => Some(Value::String(value.clone())),
        ExprKind::IntLiteral(value) => Some(Value::Int(*value)),
        ExprKind::BoolLiteral(value) => Some(Value::Bool(*value)),
        ExprKind::Null => Some(Value::Null),
        ExprKind::Variable(name) => locals.get(name).cloned(),
        ExprKind::PropertyAccess { object, .. } if matches!(object.kind, ExprKind::This) => Some(Value::Null),
        ExprKind::ClassConstant { receiver } => match receiver {
            // A generic receiver is instantiated into an ordinary named one before type checking;
            // a template has no class to reach through.
            StaticReceiver::Generic(_) => unreachable!(
                "StaticReceiver::Generic must be instantiated by generics::classes"
            ),
            StaticReceiver::Static => Some(Value::String(root.to_string())),
            StaticReceiver::Self_ => Some(Value::String(owner.to_string())),
            StaticReceiver::Named(name) => Some(Value::String(
                name.as_canonical().trim_start_matches('\\').to_string(),
            )),
            StaticReceiver::Parent => classes
                .classes
                .get(&crate::names::php_symbol_key(owner))
                .and_then(|class| class.parent.clone())
                .map(Value::String),
        },
        ExprKind::Assignment { target, value, .. } => {
            let value = evaluate_expr(classes, root, owner, value, locals, active)?;
            if let ExprKind::Variable(name) = &target.kind {
                locals.insert(name.clone(), value.clone());
            }
            Some(value)
        }
        ExprKind::NullCoalesce { value, default } => {
            let value = evaluate_expr(classes, root, owner, value, locals, active)?;
            if value == Value::Null {
                evaluate_expr(classes, root, owner, default, locals, active)
            } else {
                Some(value)
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match evaluate_expr(classes, root, owner, condition, locals, active)? {
            Value::Bool(true) => evaluate_expr(classes, root, owner, then_expr, locals, active),
            Value::Bool(false) => evaluate_expr(classes, root, owner, else_expr, locals, active),
            _ => None,
        },
        ExprKind::BinaryOp { left, op, right } => {
            let left = evaluate_expr(classes, root, owner, left, locals, active)?;
            let right = evaluate_expr(classes, root, owner, right, locals, active)?;
            match op {
                BinOp::Concat => match (left, right) {
                    (Value::String(left), Value::String(right)) => Some(Value::String(left + &right)),
                    _ => None,
                },
                BinOp::Add => match (left, right) {
                    (Value::Int(left), Value::Int(right)) => Some(Value::Int(left + right)),
                    _ => None,
                },
                BinOp::StrictEq | BinOp::Eq => Some(Value::Bool(left == right)),
                _ => None,
            }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } if args.is_empty() && matches!(object.kind, ExprKind::This) => {
            evaluate_method(classes, root, method, active)
        }
        ExprKind::FunctionCall { name, args } => {
            let canonical = name.as_canonical();
            match canonical.trim_start_matches('\\') {
                "strrpos" => evaluate_strrpos(classes, root, owner, args, locals, active),
                "substr" => evaluate_substr(classes, root, owner, args, locals, active),
                "preg_replace" => evaluate_preg_replace(classes, root, owner, args, locals, active),
                "substr_replace" => evaluate_substr_replace(classes, root, owner, args, locals, active),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Evaluates `strrpos()` for static class-string derivation.
fn evaluate_strrpos(
    classes: &ClassIndex<'_>,
    root: &str,
    owner: &str,
    args: &[Expr],
    locals: &mut HashMap<String, Value>,
    active: &mut HashSet<String>,
) -> Option<Value> {
    let [haystack, needle, ..] = args else {
        return None;
    };
    let Value::String(haystack) = evaluate_expr(classes, root, owner, haystack, locals, active)? else {
        return None;
    };
    let Value::String(needle) = evaluate_expr(classes, root, owner, needle, locals, active)? else {
        return None;
    };
    haystack.rfind(&needle).map(|index| Value::Int(index as i64))
}

/// Evaluates `substr_replace()` for a non-negative offset and an omitted or non-negative length.
fn evaluate_substr_replace(
    classes: &ClassIndex<'_>,
    root: &str,
    owner: &str,
    args: &[Expr],
    locals: &mut HashMap<String, Value>,
    active: &mut HashSet<String>,
) -> Option<Value> {
    let [subject, replacement, offset, rest @ ..] = args else {
        return None;
    };
    let Value::String(subject) = evaluate_expr(classes, root, owner, subject, locals, active)? else {
        return None;
    };
    let Value::String(replacement) = evaluate_expr(classes, root, owner, replacement, locals, active)? else {
        return None;
    };
    let Value::Int(offset) = evaluate_expr(classes, root, owner, offset, locals, active)? else {
        return None;
    };
    let start = usize::try_from(offset).ok()?.min(subject.len());
    let end = match rest.first() {
        Some(length) => {
            let Value::Int(length) = evaluate_expr(classes, root, owner, length, locals, active)? else {
                return None;
            };
            start.checked_add(usize::try_from(length).ok()?)?.min(subject.len())
        }
        None => subject.len(),
    };
    Some(Value::String(format!(
        "{}{}{}",
        subject.get(..start)?,
        replacement,
        subject.get(end..)?
    )))
}

/// Evaluates `substr()` for non-negative byte offsets in static class strings.
fn evaluate_substr(
    classes: &ClassIndex<'_>,
    root: &str,
    owner: &str,
    args: &[Expr],
    locals: &mut HashMap<String, Value>,
    active: &mut HashSet<String>,
) -> Option<Value> {
    let [subject, start, rest @ ..] = args else {
        return None;
    };
    let Value::String(subject) = evaluate_expr(classes, root, owner, subject, locals, active)? else {
        return None;
    };
    let Value::Int(start) = evaluate_expr(classes, root, owner, start, locals, active)? else {
        return None;
    };
    let start = usize::try_from(start).ok()?;
    let end = if let Some(length) = rest.first() {
        let Value::Int(length) = evaluate_expr(classes, root, owner, length, locals, active)? else {
                return None;
            };
        start.checked_add(usize::try_from(length).ok()?)?
    } else {
        subject.len()
    };
    subject.get(start..end).map(|value| Value::String(value.to_string()))
}

/// Evaluates the safe literal-suffix subset of `preg_replace()` used in deterministic class names.
fn evaluate_preg_replace(
    classes: &ClassIndex<'_>,
    root: &str,
    owner: &str,
    args: &[Expr],
    locals: &mut HashMap<String, Value>,
    active: &mut HashSet<String>,
) -> Option<Value> {
    let [pattern, replacement, subject, ..] = args else {
        return None;
    };
    let Value::String(pattern) = evaluate_expr(classes, root, owner, pattern, locals, active)? else {
        return None;
    };
    let Value::String(replacement) = evaluate_expr(classes, root, owner, replacement, locals, active)? else {
        return None;
    };
    let Value::String(subject) = evaluate_expr(classes, root, owner, subject, locals, active)? else {
        return None;
    };
    let suffix = literal_regex_suffix(&pattern)?;
    if replacement.contains(['$', '\\']) {
        return None;
    }
    let value = subject
        .strip_suffix(suffix)
        .map(|prefix| format!("{prefix}{replacement}"))
        .unwrap_or(subject);
    Some(Value::String(value))
}

/// Extracts a non-empty literal suffix from a delimited `/suffix$/` regular-expression pattern.
fn literal_regex_suffix(pattern: &str) -> Option<&str> {
    let delimiter = pattern.chars().next()?;
    if !matches!(delimiter, '/' | '#' | '~' | '%') || !pattern.ends_with(delimiter) {
        return None;
    }
    let body = &pattern[delimiter.len_utf8()..pattern.len() - delimiter.len_utf8()];
    let suffix = body.strip_suffix('$')?;
    if suffix.is_empty() || suffix.contains(['\\', '.', '*', '+', '?', '(', ')', '[', ']', '{', '}', '|', '^', '$']) {
        return None;
    }
    Some(suffix)
}

#[cfg(test)]
mod tests {
    fn candidates_of(source: &str) -> Vec<String> {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        // As in the pipeline, which resolves names before the autoload walk.
        let program = crate::name_resolver::resolve(program).expect("resolve names");
        super::candidates(&program)
    }

    /// symfony/validator's `Constraint::validatedBy()` returns `static::class.'Validator'`, and
    /// the constraint validator factory builds `new $className()` from it: each concrete
    /// constraint names its validator through the inherited return.
    #[test]
    fn a_returned_static_class_string_names_each_concrete_subclass_s_class() {
        let found = candidates_of(
            "<?php\nnamespace Lib;\nabstract class Constraint { public function validatedBy(): string { return static::class.'Validator'; } }\nclass NotBlank extends Constraint {}\nabstract class Partial extends Constraint {}\n",
        );
        assert!(found.contains(&"Lib\\NotBlankValidator".to_string()), "{found:?}");
        assert!(!found.contains(&"Lib\\PartialValidator".to_string()), "abstract classes name nothing: {found:?}");
    }

    /// A return that does not derive from `static::class` adds nothing.
    #[test]
    fn an_ordinary_returned_string_is_no_candidate() {
        let found = candidates_of(
            "<?php\nnamespace Lib;\nclass Plain { public function name(): string { return 'Lib\\\\Other'; } }\n",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// A class name concatenated from literals names each class a ternary branch can spell, inside
    /// a method too; a concatenation over a runtime value names nothing.
    #[test]
    fn a_concatenated_class_string_names_each_branch() {
        let found = candidates_of(
            "<?php\n$name = 'Acme\\\\Lib\\\\' . ($argc > 5 ? 'Other' : 'Listed');\nclass F { function m($x) { return 'Acme\\\\Rule\\\\' . ($x ? 'A' : 'B') . 'Rule'; } function n($x) { return 'Acme\\\\' . $x; } }\n",
        );
        for name in ["Acme\\Lib\\Other", "Acme\\Lib\\Listed", "Acme\\Rule\\ARule", "Acme\\Rule\\BRule"] {
            assert!(found.contains(&name.to_string()), "{name} missing: {found:?}");
        }
        assert_eq!(found.len(), 4, "{found:?}");
    }
}
