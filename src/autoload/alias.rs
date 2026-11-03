//! Purpose:
//! Collects top-level `class_alias("Original", "Alias")` calls whose class names are
//! compile-time constants.
//! Synthesizes subclass declarations that approximate alias use in the AOT class table.
//!
//! Called from:
//! - `crate::autoload::registry::Registry::build()`
//! - `crate::autoload::collect_aliases()` after include/autoload expansion
//!
//! Key details:
//! - A post-name-resolution pass also recognizes `Original::class` and `Alias::class`
//!   in nested statement lists while leaving the defensive call in place.
//! - A class-name argument may be a string literal, a `Name::class` constant, or a `.`
//!   concatenation of those. `Name::class` is resolved here, before the name resolver runs,
//!   against the namespace and class imports in effect at the call, tracked with the name
//!   resolver's own rules so both passes agree on the name.
//! - Runtime-dynamic alias calls are left in the program and rejected by the checker.
//! - Disabled autoload is diagnosed on eligible source forms before constant folding.
//! - Resolver-created include wrappers still count as top-level for included-file aliases.
//! - The alias is a subclass, not a true PHP runtime alias, so identity checks differ in documented cases.

use std::collections::{HashMap, HashSet};


use crate::names::{php_symbol_key, Name, NameKind};
use crate::parser::ast::{
    BinOp, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, UseKind,
};

/// The namespace and class imports that apply to one top-level statement.
///
/// Mirrors `crate::name_resolver::statements::list::resolve_stmt_list`: a `namespace X;`
/// statement switches the namespace and drops the imports for the statements after it, a
/// namespace block starts from no imports, `use` adds class imports for the statements after
/// it, and a nested wrapper list (an included file) starts from the enclosing context without
/// leaking its own changes back out.
#[derive(Clone, Default)]
struct AliasScope {
    /// Current namespace, `None` or empty for the global namespace.
    namespace: Option<String>,
    /// Class imports keyed by the lowercased alias, mapping to the imported fully qualified name.
    class_imports: HashMap<String, String>,
}

/// Walk top-level statements for `class_alias("Orig", "Alias")` calls
/// (with compile-time-constant arguments). Strip every collected call and append a
/// synthesized `class Alias extends Orig {}` declaration. Calls with
/// runtime-dependent arguments stay in the program and are rejected by the checker.
pub fn collect_aliases(program: Program) -> Program {
    let mut alias_decls: Vec<Stmt> = Vec::new();
    let mut cleaned =
        collect_aliases_in_top_level(program, AliasScope::default(), &mut alias_decls);
    cleaned.extend(alias_decls);
    cleaned
}

/// Collects aliases whose class-string arguments became static only after name resolution.
///
/// Calls remain in their original statement lists so their boolean result and control-flow
/// position stay visible. The synthesized declaration makes the alias available to the closed
/// world class table before type checking; a retained call therefore lowers as the defensive
/// "already defined" result.
pub fn collect_resolved_aliases(mut program: Program) -> Program {
    let mut declared = super::walk::collect_declared_fqns(&program)
        .into_iter()
        .map(|name| php_symbol_key(&name))
        .collect::<HashSet<_>>();
    let mut alias_decls = Vec::new();
    collect_resolved_aliases_in_program(&mut program, &mut alias_decls, &mut declared);
    // An alias of an INTERFACE is an interface: nette/utils' `class_alias(Translator::class,
    // ITranslator::class)` became `class ITranslator extends Translator`, which the checker
    // rightly refuses ("cannot extend interface"), and the file went to the interpreter.
    let interfaces = declared_interface_keys(&program);
    program.extend(alias_decls.into_iter().map(|decl| alias_as_interface(decl, &interfaces)));
    program
}

/// The lowercased fully qualified names of the interfaces `program` declares at file scope.
fn declared_interface_keys(program: &Program) -> HashSet<String> {
    fn walk(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::InterfaceDecl { name, .. } => {
                    out.insert(php_symbol_key(name.trim_start_matches('\\')));
                }
                StmtKind::NamespaceBlock { body, .. }
                | StmtKind::IncludeOnceGuard { body, .. }
                | StmtKind::Synthetic(body) => walk(body, out),
                _ => {}
            }
        }
    }
    let mut out = HashSet::new();
    walk(program, &mut out);
    out
}

/// Turns a synthesized `class Alias extends Original {}` into `interface Alias extends Original {}`
/// when `Original` is one of `interfaces`; every other declaration is returned unchanged.
fn alias_as_interface(decl: Stmt, interfaces: &HashSet<String>) -> Stmt {
    let StmtKind::ClassDecl { name, extends: Some(parent), .. } = &decl.kind else {
        return decl;
    };
    if !interfaces.contains(&php_symbol_key(parent.as_canonical().trim_start_matches('\\'))) {
        return decl;
    }
    Stmt::new(
        StmtKind::InterfaceDecl {
            name: name.clone(),
            generics: None,
            extends: vec![parent.clone()],
            properties: Vec::new(),
            methods: Vec::new(),
            constants: Vec::new(),
        },
        decl.span,
    )
}

/// Scans one resolved statement list for static alias calls and nested statement lists.
fn collect_resolved_aliases_in_program(
    program: &mut Program,
    alias_decls: &mut Vec<Stmt>,
    declared: &mut HashSet<String>,
) {
    for stmt in program {
        collect_resolved_aliases_in_stmt(stmt, alias_decls, declared);
    }
}

/// Records a resolved alias call and descends through every statement-owned body.
fn collect_resolved_aliases_in_stmt(
    stmt: &mut Stmt,
    alias_decls: &mut Vec<Stmt>,
    declared: &mut HashSet<String>,
) {
    if let Some((original, alias)) = extract_resolved_class_alias(stmt) {
        if declared.insert(php_symbol_key(&alias)) {
            alias_decls.push(synthesise_resolved_alias_decl(
                &original,
                &alias,
                stmt.span,
            ));
        }
    }

    match &mut stmt.kind {
        StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. }
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::FunctionDecl { body, .. } => {
            collect_resolved_aliases_in_program(body, alias_decls, declared);
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            collect_resolved_aliases_in_program(then_body, alias_decls, declared);
            for (_, body) in elseif_clauses {
                collect_resolved_aliases_in_program(body, alias_decls, declared);
            }
            if let Some(body) = else_body {
                collect_resolved_aliases_in_program(body, alias_decls, declared);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            collect_resolved_aliases_in_program(then_body, alias_decls, declared);
            if let Some(body) = else_body {
                collect_resolved_aliases_in_program(body, alias_decls, declared);
            }
        }
        StmtKind::For {
            init, update, body, ..
        } => {
            if let Some(init) = init {
                collect_resolved_aliases_in_stmt(init, alias_decls, declared);
            }
            if let Some(update) = update {
                collect_resolved_aliases_in_stmt(update, alias_decls, declared);
            }
            collect_resolved_aliases_in_program(body, alias_decls, declared);
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                collect_resolved_aliases_in_program(body, alias_decls, declared);
            }
            if let Some(body) = default {
                collect_resolved_aliases_in_program(body, alias_decls, declared);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            collect_resolved_aliases_in_program(try_body, alias_decls, declared);
            for catch in catches {
                collect_resolved_aliases_in_program(&mut catch.body, alias_decls, declared);
            }
            if let Some(body) = finally_body {
                collect_resolved_aliases_in_program(body, alias_decls, declared);
            }
        }
        StmtKind::ClassDecl { methods, .. }
        | StmtKind::TraitDecl { methods, .. }
        | StmtKind::InterfaceDecl { methods, .. }
        | StmtKind::EnumDecl { methods, .. } => {
            for method in methods {
                collect_resolved_aliases_in_program(&mut method.body, alias_decls, declared);
            }
        }
        _ => {}
    }
}

/// Iterates over top-level statements, removing each `class_alias("Orig", "Alias")`
/// call with constant arguments and appending the corresponding synthesized
/// `class Alias extends Orig {}` declaration to `alias_decls`. Returns the
/// filtered statements; `collect_aliases` appends the collected declarations at the end.
/// Runtime-dependent `class_alias` calls remain in the program and are not
/// collected; the caller is responsible for rejecting them.
fn collect_aliases_in_top_level(
    program: Program,
    mut scope: AliasScope,
    alias_decls: &mut Vec<Stmt>,
) -> Program {
    program
        .into_iter()
        .filter_map(|stmt| collect_aliases_in_stmt(stmt, &mut scope, alias_decls))
        .collect()
}

/// Inspects a single statement for a `class_alias` call. If found, pushes the
/// synthesized subclass declaration to `alias_decls` and returns `None` to remove
/// the original call from the program. Updates `scope` for namespace and `use`
/// statements, descends into `NamespaceBlock`, `IncludeOnceGuard`, and `Synthetic`
/// wrappers, and returns every other statement kind unchanged after the alias check.
fn collect_aliases_in_stmt(
    stmt: Stmt,
    scope: &mut AliasScope,
    alias_decls: &mut Vec<Stmt>,
) -> Option<Stmt> {
    if let Some((orig, alias)) = extract_class_alias(&stmt, scope) {
        alias_decls.push(synthesise_alias_decl(&orig, &alias, stmt.span));
        return None;
    }

    let span = stmt.span;
    let source_mode = stmt.source_mode;
    let strict_types = stmt.strict_types;
    let attributes = stmt.attributes;
    let kind = match stmt.kind {
        StmtKind::NamespaceDecl { name } => {
            scope.namespace = Some(namespace_text(&name));
            scope.class_imports.clear();
            StmtKind::NamespaceDecl { name }
        }
        StmtKind::UseDecl { imports } => {
            for item in imports.iter().filter(|item| item.kind == UseKind::Class) {
                scope
                    .class_imports
                    .insert(php_symbol_key(&item.alias), item.name.as_canonical());
            }
            StmtKind::UseDecl { imports }
        }
        StmtKind::NamespaceBlock { name, body } => {
            let block_scope = AliasScope {
                namespace: Some(namespace_text(&name)),
                class_imports: HashMap::new(),
            };
            StmtKind::NamespaceBlock {
                name,
                body: collect_aliases_in_top_level(body, block_scope, alias_decls),
            }
        }
        StmtKind::IncludeOnceGuard { source_path, required, body } => StmtKind::IncludeOnceGuard {
            source_path,
            required,
            body: collect_aliases_in_top_level(body, scope.clone(), alias_decls),
        },
        StmtKind::Synthetic(body) => {
            StmtKind::Synthetic(collect_aliases_in_top_level(body, scope.clone(), alias_decls))
        }
        kind => kind,
    };
    Some(Stmt {
        kind,
        span,
        source_mode,
        strict_types,
        attributes,
    })
}

/// Returns the canonical text of a namespace declaration name, empty for the global namespace.
fn namespace_text(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

/// Extract class alias pair from a statement if it is a `class_alias` call whose class names
/// are compile-time constants.
fn extract_class_alias(stmt: &Stmt, scope: &AliasScope) -> Option<(String, String)> {
    let args = positional_class_alias_args(stmt)?;
    if let Some(autoload_arg) = args.get(2) {
        match &autoload_arg.kind {
            ExprKind::BoolLiteral(true) => {}
            ExprKind::IntLiteral(n) if *n != 0 => {}
            _ => return None,
        }
    }
    let orig = constant_class_name(args.first()?, scope)?;
    let alias = constant_class_name(args.get(1)?, scope)?;
    Some((orig, alias))
}

/// Shares the collector's direct positional call shape with diagnostic eligibility.
fn positional_class_alias_args(stmt: &Stmt) -> Option<&[Expr]> {
    let StmtKind::ExprStmt(expr) = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    let canonical = name.as_canonical();
    if !canonical
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("class_alias")
    {
        return None;
    }
    if !class_alias_arity_is_supported(args) {
        return None;
    }
    Some(args)
}

/// Checks the argument shape of a `class_alias()` call the compiler can resolve statically.
///
/// The third argument is PHP's `$autoload`: whether to AUTOLOAD the original class before
/// aliasing it. It says nothing about whether the two names are statically known, so a literal
/// `false` is just as resolvable as a literal `true` — in a closed world the original is either
/// compiled in or the alias is rejected anyway, and autoloading never enters into it.
///
/// Refusing `false` is what kept Symfony's generated container out of the compiled world: it
/// emits `\class_alias(\ContainerXXXX\App_KernelProdContainer::class, App_KernelProdContainer::class, false)`
/// — both names `::class` constants — and the whole entry failed with
/// "class_alias() requires statically resolvable class names".
///
/// A NON-literal flag is still refused: it is a value the compiler cannot see, and accepting it
/// would mean guessing at a call it cannot model.
///
/// Named and spread arguments are refused outright: the collector reads the call positionally.
fn class_alias_arity_is_supported(args: &[Expr]) -> bool {
    if args.len() < 2
        || args.len() > 3
        || args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
    {
        return false;
    }
    match args.get(2).map(|arg| &arg.kind) {
        None => true,
        Some(ExprKind::BoolLiteral(_)) => true,
        Some(ExprKind::IntLiteral(_)) => true,
        Some(_) => false,
    }
}

/// Extracts a class alias pair from a resolved statement call with static class strings.
fn extract_resolved_class_alias(stmt: &Stmt) -> Option<(String, String)> {
    let StmtKind::ExprStmt(expr) = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if !name
        .as_canonical()
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("class_alias")
    {
        return None;
    }
    resolved_class_alias_args(args)
}

/// Returns the statically known original and alias class names for supported call arguments.
pub(crate) fn resolved_class_alias_args(args: &[Expr]) -> Option<(String, String)> {
    if !class_alias_arity_is_supported(args) {
        return None;
    }
    Some((
        resolved_class_name(args.first()?)?,
        resolved_class_name(args.get(1)?)?,
    ))
}

/// Resolves a literal string or a canonical named `ClassName::class` expression.
fn resolved_class_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(name) => Some(name.trim_start_matches('\\').to_string()),
        ExprKind::ClassConstant {
            receiver: crate::parser::ast::StaticReceiver::Named(name),
        } => Some(name.as_canonical().trim_start_matches('\\').to_string()),
        _ => None,
    }
}

/// Folds a compile-time-constant class-name argument to its string value.
///
/// Accepts a string literal (already fully qualified, like every PHP class-name string), a
/// `Name::class` constant, and a `.` concatenation of those, which also covers
/// `__NAMESPACE__ . '\Alias'` because magic constants are substituted before this pass.
/// Anything else (variables, `$object::class`, calls, class constants) returns `None`, so the
/// call stays in the program and keeps the checker's diagnostic.
fn constant_class_name(expr: &Expr, scope: &AliasScope) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(s) => Some(s.clone()),
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => Some(resolve_class_reference(name, scope)),
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let mut folded = constant_class_name(left, scope)?;
            folded.push_str(&constant_class_name(right, scope)?);
            Some(folded)
        }
        _ => None,
    }
}

/// Resolves the class name of a `Name::class` constant to the fully qualified name PHP
/// compiles it to.
///
/// Follows `crate::name_resolver::names::resolved_class_name` without its symbol-table
/// spelling pass (the synthesized declaration goes through the name resolver afterwards): a
/// fully qualified name is taken as written, an unqualified name through a class import of the
/// same alias, a qualified name through an import of its first segment, and anything else is
/// placed in the current namespace.
fn resolve_class_reference(name: &Name, scope: &AliasScope) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(target) = name
            .last_segment()
            .and_then(|segment| scope.class_imports.get(&php_symbol_key(segment)))
        {
            return target.clone();
        }
    } else if let Some(target) = name
        .parts
        .first()
        .and_then(|first| scope.class_imports.get(&php_symbol_key(first)))
    {
        let suffix = &name.parts[1..];
        return if suffix.is_empty() {
            target.clone()
        } else {
            format!("{}\\{}", target, suffix.join("\\"))
        };
    }
    match scope.namespace.as_deref() {
        Some(namespace) if !namespace.is_empty() => {
            format!("{}\\{}", namespace, name.as_canonical())
        }
        _ => name.as_canonical(),
    }
}

/// Synthesize `class Alias extends Original {}` for the given pair of
/// FQNs. The declaration is always wrapped in a `NamespaceBlock` for the alias
/// name's own namespace (the global one when it has none): the declarations are
/// appended after the program's last statement, which may sit under a statement-form
/// `namespace X;` that must not capture a global alias name.
fn synthesise_alias_decl(orig: &str, alias: &str, span: crate::span::Span) -> Stmt {
    let orig_parts: Vec<String> = orig
        .trim_start_matches('\\')
        .split('\\')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let alias_parts: Vec<String> = alias
        .trim_start_matches('\\')
        .split('\\')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let alias_local = alias_parts.last().cloned().unwrap_or_default();
    let alias_namespace_parts = alias_parts
        .iter()
        .take(alias_parts.len().saturating_sub(1))
        .cloned()
        .collect::<Vec<_>>();

    let extends_name = Name::from_parts(NameKind::FullyQualified, orig_parts);

    let class_stmt = Stmt::new(
        StmtKind::ClassDecl {
            generics: None,
            name: alias_local,
            doc_comment: None,
            extends: Some(extends_name),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: Vec::new(),
            constants: Vec::new(),
        },
        span,
    );

    let ns_name = (!alias_namespace_parts.is_empty())
        .then(|| Name::from_parts(NameKind::Qualified, alias_namespace_parts));
    Stmt::new(
        StmtKind::NamespaceBlock {
            name: ns_name,
            body: vec![class_stmt],
        },
        span,
    )
}

/// Synthesizes an alias declaration from already-canonical class names.
fn synthesise_resolved_alias_decl(
    orig: &str,
    alias: &str,
    span: crate::span::Span,
) -> Stmt {
    let orig_parts = orig
        .trim_start_matches('\\')
        .split('\\')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    Stmt::new(
        StmtKind::ClassDecl {
            generics: None,
            name: alias.trim_start_matches('\\').to_string(),
            doc_comment: None,
            extends: Some(Name::from_parts(NameKind::FullyQualified, orig_parts)),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: Vec::new(),
            constants: Vec::new(),
        },
        span,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses one direct alias call without changing its source positions.
    fn alias_statement() -> Stmt {
        let tokens = crate::lexer::tokenize("<?php class_alias('Original', 'Alias', false);").unwrap();
        crate::parser::parse(&tokens).unwrap().remove(0)
    }

    /// A literal `autoload = false` is a statically resolvable alias, not a diagnostic: the
    /// original is either compiled into the closed world or the alias is rejected anyway.
    #[test]
    fn literal_autoload_false_resolves_statically_through_wrappers() {
        let call = alias_statement();
        let span = call.span;
        let program = vec![
            call.clone(),
            Stmt::new(StmtKind::Synthetic(vec![call.clone()]), span),
            Stmt::new(StmtKind::IncludeOnceGuard { source_path: "review_include".into(), required: true, body: vec![call] }, span),
        ];
        for stmt in &program {
            let direct = match &stmt.kind {
                StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => &body[0],
                _ => stmt,
            };
            assert_eq!(direct.span, span);
            let args = positional_class_alias_args(direct).expect("direct positional call");
            assert_eq!(
                resolved_class_alias_args(args),
                Some(("Original".to_string(), "Alias".to_string())),
            );
        }
    }

    /// Named and spread arguments never reach the positional collector.
    #[test]
    fn named_and_spread_arguments_are_not_collectible() {
        let tokens = crate::lexer::tokenize("<?php class_alias(original: 'Original', alias: 'Alias');").unwrap();
        let call = crate::parser::parse(&tokens).unwrap().remove(0);
        assert!(positional_class_alias_args(&call).is_none());
    }
}
