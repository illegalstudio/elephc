//! Purpose:
//! Compiles a class file whose file-scope code php runs only when one of its classes is first
//! loaded, for a closed world: the declarations are compiled as usual, the file-scope statements
//! become a guarded function, and every use of such a class that php would autoload it for calls
//! that function first.
//!
//! Called from:
//! - `crate::autoload::load_autoloaded_bundle` (`hoist_file_scope_effects`), in place of handing
//!   the file to the runtime autoloader, which a closed world has none of.
//! - `crate::pipeline` after name resolution (`call_lazy_initializers`).
//!
//! Key details:
//! - php includes an autoloaded file from inside the autoloader function, so its file-scope
//!   variables are that function's locals: a function body is their scope here too. psysh's Hoa
//!   readline files end with `register_shutdown_function(...)` or `Console::advancedInteraction()`,
//!   php-parser's `Lexer.php` with the call that defines its compatibility tokens.
//! - Each class-like the file declares gets `<namespace>\__elephc_lazy_init_<class>()`, which runs
//!   the file's statements once (a static guard) and answers null. A class whose ancestor has one
//!   runs the ancestor's first, as php loads a parent file while declaring the child.
//! - A use is `new C`, `C::m()`, `C::$p` or `C::K` with a literal class name, rewritten to
//!   `__elephc_lazy_init_c() ?? <use>`, so the initializer runs where php's autoload would and the
//!   use keeps its value. `instanceof`, type declarations and `C::class` load nothing in php and are
//!   left alone.
//! - Declarations stay where they are, and so does an `if` whose branches only declare (a guarded
//!   polyfill function); a file with a nested declaration among its effects is not taken.

use std::collections::{HashMap, HashSet};

use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind};
use crate::span::Span;

/// The initializer function's name prefix; the class name, case-folded, follows it.
const INIT_PREFIX: &str = "__elephc_lazy_init_";

/// Whether `stmt` only declares: php binds it with the file, ahead of or regardless of the file's
/// effects.
fn declares_only(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::ClassDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::FunctionDecl { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. }
        | StmtKind::ConstDecl { .. } => true,
        StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => {
            body.iter().all(declares_only)
        }
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            then_body.iter().all(declares_only)
                && elseif_clauses.iter().all(|(_, body)| body.iter().all(declares_only))
                && else_body.as_deref().is_none_or(|body| body.iter().all(declares_only))
        }
        _ => false,
    }
}

/// Whether `stmt` declares anything at any depth (a function or class-like body excluded).
fn nests_a_declaration(stmt: &Stmt) -> bool {
    let mut found = false;
    crate::parser::ast::visit::walk_stmts(std::slice::from_ref(stmt), &mut |inner| {
        found |= matches!(
            inner.kind,
            StmtKind::ClassDecl { .. }
                | StmtKind::InterfaceDecl { .. }
                | StmtKind::TraitDecl { .. }
                | StmtKind::EnumDecl { .. }
                | StmtKind::FunctionDecl { .. }
        );
    });
    found
}

/// Whether `stmt` is `if (!function_exists('F')) { function F() {...} <effects> }` whose body
/// declares F and nests no other declaration in its effects: php-parser's
/// `compatibility_tokens.php` declares `defineCompatibilityTokens()` and calls it under that
/// guard. The declaration is compiled unconditionally and the effects join the file's initializer:
/// the guard only fails when another file declares F, which a closed world refuses as a duplicate
/// declaration. Kept under its guard, F was a conditional declaration the initializer's call could
/// not reach.
fn self_guarded_function_with_effects(stmt: &Stmt) -> bool {
    let StmtKind::If { condition, then_body, elseif_clauses, else_body } = &stmt.kind else {
        return false;
    };
    if !elseif_clauses.is_empty() || else_body.is_some() {
        return false;
    }
    let ExprKind::Not(inner) = &condition.kind else {
        return false;
    };
    let ExprKind::FunctionCall { name, args } = &inner.kind else {
        return false;
    };
    let [arg] = args.as_slice() else {
        return false;
    };
    let ExprKind::StringLiteral(guarded) = &arg.kind else {
        return false;
    };
    if !name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("function_exists") {
        return false;
    }
    let guarded = php_symbol_key(guarded.trim_start_matches('\\'));
    let declares_it = then_body.iter().any(|inner| {
        matches!(&inner.kind, StmtKind::FunctionDecl { name, .. }
            if php_symbol_key(name.trim_start_matches('\\')) == guarded
                || php_symbol_key(name.rsplit('\\').next().unwrap_or(name))
                    == php_symbol_key(guarded.rsplit('\\').next().unwrap_or(&guarded)))
    });
    declares_it
        && then_body
            .iter()
            .filter(|inner| !declares_only(inner))
            .all(|inner| !nests_a_declaration(inner))
}

/// Splits one statement list into what stays in place and the file-scope effects, in order.
/// `Synthetic` bodies (spliced requires) are split recursively. `None` when an effect nests a
/// declaration, which cannot be moved into a function.
fn split(body: Vec<Stmt>, effects: &mut Vec<Stmt>, classes: &mut Vec<String>) -> Option<Vec<Stmt>> {
    let mut kept = Vec::with_capacity(body.len());
    for stmt in body {
        match stmt.kind {
            StmtKind::ClassDecl { ref name, .. }
            | StmtKind::InterfaceDecl { ref name, .. }
            | StmtKind::TraitDecl { ref name, .. }
            | StmtKind::EnumDecl { ref name, .. } => {
                classes.push(name.clone());
                kept.push(stmt);
            }
            StmtKind::Synthetic(inner) => {
                let inner = split(inner, effects, classes)?;
                kept.push(Stmt { kind: StmtKind::Synthetic(inner), ..stmt });
            }
            _ if declares_only(&stmt) => kept.push(stmt),
            _ if self_guarded_function_with_effects(&stmt) => {
                let StmtKind::If { then_body, .. } = stmt.kind else { unreachable!() };
                let (declarations, rest): (Vec<Stmt>, Vec<Stmt>) =
                    then_body.into_iter().partition(declares_only);
                kept.extend(declarations);
                effects.extend(rest);
            }
            _ if nests_a_declaration(&stmt) => return None,
            _ => effects.push(stmt),
        }
    }
    Some(kept)
}

/// The short (last-segment) name of a class-like, case-folded, for its initializer's name.
fn init_suffix(class: &str) -> String {
    php_symbol_key(class.trim_start_matches('\\').rsplit('\\').next().unwrap_or(class))
}

/// The initializer's fully qualified name for a class-like's fully qualified name: the class's
/// namespace, then the prefix and the short name. A loaded file is already name-resolved, so the
/// declaration carries this resolved name itself.
fn init_name(class: &str) -> String {
    match class.trim_start_matches('\\').rsplit_once('\\') {
        Some((namespace, _)) => format!("{namespace}\\{INIT_PREFIX}{}", init_suffix(class)),
        None => format!("{INIT_PREFIX}{}", init_suffix(class)),
    }
}

/// The guarded initializer: `static $done = false; if ($done) return null; $done = true; ...`.
fn initializer(name: &str, effects: Vec<Stmt>) -> Stmt {
    use crate::synthetic_class::{e_bool, e_null, e_var, function, s_assign, s_if, s_return, s_static};
    let mut body = vec![
        s_static("__elephc_lazy_done", e_bool(false)),
        s_if(e_var("__elephc_lazy_done"), vec![s_return(e_null())], vec![], None),
        s_assign("__elephc_lazy_done", e_bool(true)),
    ];
    body.extend(effects);
    body.push(s_return(e_null()));
    function(name).body(body).build()
}

/// A call of the resolved function `name`, with no arguments.
fn resolved_call(name: &str) -> Expr {
    let parts = name.trim_start_matches('\\').split('\\').map(str::to_string).collect();
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(crate::names::NameKind::FullyQualified, parts),
            args: Vec::new(),
        },
        Span::dummy(),
    )
}

/// `<name>()` delegating one class's initializer to the file's.
fn delegate(name: &str, target: &str) -> Stmt {
    use crate::synthetic_class::{function, s_return};
    function(name).body(vec![s_return(resolved_call(target))]).build()
}

/// Rewrites a loaded class file so its file-scope effects run on the first use of one of its
/// classes (see the module notes). `None` when the file is not one this can take.
pub(super) fn hoist_file_scope_effects(program: Program) -> Option<Program> {
    // The file's statements, or its one braced namespace's.
    let (mut namespace_block, body) = match program.as_slice() {
        [single] if matches!(single.kind, StmtKind::NamespaceBlock { .. }) => {
            let mut program = program;
            let block = program.remove(0);
            let StmtKind::NamespaceBlock { ref body, .. } = block.kind else { unreachable!() };
            let body = body.clone();
            (Some(block), body)
        }
        _ => (None, program),
    };
    let mut effects = Vec::new();
    let mut classes = Vec::new();
    let mut kept = split(body, &mut effects, &mut classes)?;
    if effects.is_empty() || classes.is_empty() {
        return None;
    }
    let owner = init_name(&classes[0]);
    kept.push(initializer(&owner, effects));
    for class in &classes[1..] {
        kept.push(delegate(&init_name(class), &owner));
    }
    Some(match namespace_block.take() {
        Some(mut block) => {
            if let StmtKind::NamespaceBlock { ref mut body, .. } = block.kind {
                *body = kept;
            }
            vec![block]
        }
        None => kept,
    })
}

/// Takes a file-scope `if (<cond>) { <class-likes> }` (no other arm) out of its condition, for a
/// closed world: the class-likes are compiled, and the condition becomes a file-scope guard that
/// raises php's `Class "X" not found` when it does not hold. Hoisted with the file's other
/// file-scope code ([`hoist_file_scope_effects`]), the guard runs on the first use of one of the
/// classes, where php's autoload would have included the file and found no such class.
/// nunomaduro/collision declares its PHPUnit subscriber under
/// `if (class_exists(Version::class) && (int) Version::series() >= 10)`.
///
/// `class_exists()` of such a class answers true whatever the condition: it runs no initializer.
/// Answers whether anything was taken out.
pub(super) fn unconditional_class_declarations(program: &mut Program) -> bool {
    fn class_like(stmt: &Stmt) -> Option<(&'static str, &str)> {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::EnumDecl { name, .. } => Some(("Class", name)),
            StmtKind::InterfaceDecl { name, .. } => Some(("Interface", name)),
            StmtKind::TraitDecl { name, .. } => Some(("Trait", name)),
            _ => None,
        }
    }
    fn rewrite(stmts: &mut Vec<Stmt>) -> bool {
        let mut changed = false;
        let mut out = Vec::with_capacity(stmts.len());
        for stmt in std::mem::take(stmts) {
            let taken = match &stmt.kind {
                StmtKind::If { condition, then_body, elseif_clauses, else_body }
                    if elseif_clauses.is_empty()
                        && else_body.is_none()
                        && !then_body.is_empty()
                        && then_body.iter().all(|inner| class_like(inner).is_some()) =>
                {
                    let (kind, name) = class_like(&then_body[0]).expect("checked above");
                    let message = format!("{kind} \"{}\" not found", name.trim_start_matches('\\'));
                    use crate::synthetic_class::{e_new, e_not, e_str, s_if, s_throw};
                    let guard = s_if(
                        e_not(condition.clone()),
                        vec![s_throw(e_new("\\Error", vec![e_str(&message)]))],
                        vec![],
                        None,
                    );
                    Some((then_body.clone(), guard))
                }
                _ => None,
            };
            match taken {
                Some((declarations, guard)) => {
                    out.extend(declarations);
                    out.push(guard);
                    changed = true;
                }
                None => out.push(stmt),
            }
        }
        *stmts = out;
        changed
    }
    let mut changed = false;
    for stmt in program.iter_mut() {
        if let StmtKind::NamespaceBlock { body, .. } = &mut stmt.kind {
            changed |= rewrite(body);
        }
    }
    changed | rewrite(program)
}

/// `name` qualified by the namespace it is declared in, unless it already is.
fn qualified(namespace: &str, name: &str) -> String {
    let name = name.trim_start_matches('\\');
    if namespace.is_empty() || name.contains('\\') {
        name.to_string()
    } else {
        format!("{namespace}\\{name}")
    }
}

/// Walks the declarations of `stmts` with the namespace each is declared in: autoloaded files are
/// spliced before their names are resolved, so a declaration may still be spelled short.
fn walk_declarations(stmts: &[Stmt], namespace: &mut String, visit: &mut dyn FnMut(&Stmt, &str)) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => {
                *namespace = name.as_ref().map(|name| name.as_str().trim_start_matches('\\').to_string()).unwrap_or_default();
            }
            StmtKind::NamespaceBlock { name, body } => {
                let mut inner = name.as_ref().map(|name| name.as_str().trim_start_matches('\\').to_string()).unwrap_or_default();
                walk_declarations(body, &mut inner, visit);
            }
            StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => {
                // A spliced file starts in the global namespace and restores nothing after it.
                let mut inner = namespace.clone();
                walk_declarations(body, &mut inner, visit);
            }
            _ => visit(stmt, namespace),
        }
    }
}

/// The class-likes with an initializer, by case-folded FQN, to that initializer's FQN.
fn initializers(program: &Program) -> HashMap<String, String> {
    let mut functions = HashSet::new();
    let mut classes = Vec::new();
    walk_declarations(program, &mut String::new(), &mut |stmt, namespace| match &stmt.kind {
        StmtKind::FunctionDecl { name, .. }
            if name.rsplit('\\').next().is_some_and(|short| short.starts_with(INIT_PREFIX)) =>
        {
            functions.insert(php_symbol_key(&qualified(namespace, name)));
        }
        StmtKind::ClassDecl { name, .. }
        | StmtKind::InterfaceDecl { name, .. }
        | StmtKind::TraitDecl { name, .. }
        | StmtKind::EnumDecl { name, .. } => classes.push(qualified(namespace, name)),
        _ => {}
    });
    let mut out = HashMap::new();
    if functions.is_empty() {
        return out;
    }
    for class in classes {
        let name = init_name(&class);
        if functions.contains(&php_symbol_key(&name)) {
            out.insert(php_symbol_key(&class), name);
        }
    }
    out
}

/// Each class's parent, by case-folded FQN. A parent still spelled relative to the file's
/// namespace is qualified by it (`use` imports are not consulted: a parent named through one keeps
/// its spelling, and its chain is then simply not followed).
fn parents(program: &Program) -> HashMap<String, String> {
    let mut out = HashMap::new();
    walk_declarations(program, &mut String::new(), &mut |stmt, namespace| {
        if let StmtKind::ClassDecl { name, extends: Some(parent), .. } = &stmt.kind {
            let parent = if parent.is_fully_qualified() {
                parent.as_str().trim_start_matches('\\').to_string()
            } else {
                qualified(namespace, parent.as_str())
            };
            out.insert(php_symbol_key(&qualified(namespace, name)), php_symbol_key(&parent));
        }
    });
    out
}

/// After name resolution: makes every use php would autoload a class for call the initializers of
/// that class and its ancestors first, outermost ancestor first.
pub(crate) fn call_lazy_initializers(program: &mut Program) {
    let initializers = initializers(program);
    if initializers.is_empty() {
        return;
    }
    let parents = parents(program);
    let chain = |class: &str| -> Vec<String> {
        let mut names = Vec::new();
        let mut current = Some(php_symbol_key(class.trim_start_matches('\\')));
        let mut depth = 0;
        while let Some(key) = current {
            if let Some(name) = initializers.get(&key) {
                names.push(name.clone());
            }
            depth += 1;
            if depth > 64 {
                break;
            }
            current = parents.get(&key).cloned();
        }
        names.reverse();
        names
    };
    let mut rewritten: HashSet<Span> = HashSet::new();
    crate::parser::ast::visit::walk_nodes_mut(program, &mut |node| {
        let crate::parser::ast::visit::NodeMut::Expr(expr) = node else {
            return;
        };
        // A synthetic node has no source span to tell its rewritten copy apart; none of them
        // names a class from a user file.
        if expr.span == Span::dummy() || rewritten.contains(&expr.span) {
            return;
        }
        let class = match &expr.kind {
            ExprKind::NewObject { class_name, .. } => class_name.as_str().to_string(),
            ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(name), .. }
            | ExprKind::StaticPropertyAccess { receiver: StaticReceiver::Named(name), .. }
            | ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named(name), .. } => {
                name.as_str().to_string()
            }
            _ => return,
        };
        let names = chain(&class);
        if names.is_empty() {
            return;
        }
        rewritten.insert(expr.span);
        let span = expr.span;
        let original = std::mem::replace(expr, Expr::new(ExprKind::Null, span));
        let mut value = original;
        for name in names.into_iter().rev() {
            let call = resolved_call(&name);
            value = Expr::new(
                ExprKind::NullCoalesce { value: Box::new(call), default: Box::new(value) },
                span,
            );
        }
        *expr = value;
    });
}
