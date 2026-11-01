//! Purpose:
//! Folds literal `defined("Class::CONSTANT")` probes when the closed-world AST makes the
//! answer certain before type checking.
//!
//! Called from:
//! - `crate::optimize::fold_constants()` and `crate::optimize::fold::expr::fold_expr()`.
//!
//! Key details:
//! - Inherited constants are resolved only through declarations present in the closed-world AST;
//!   an unknown dependency or a cycle keeps the probe dynamic.

use super::super::*;

/// Direct class-constant facts that are safe to answer without type-checker metadata.
#[derive(Clone, Debug, Default)]
struct KnownClassConstants {
    declarations: HashMap<String, KnownClassDeclaration>,
}

/// Constant names and whether the declaration can prove the absence of every other name.
#[derive(Clone, Debug, Default)]
struct KnownClassDeclaration {
    constants: HashSet<String>,
    /// Private and protected constants. `defined()` answers by the CALLING scope -- true inside
    /// the class, false outside -- which a fold cannot see, so a probe naming one stays open.
    scoped: HashSet<String>,
    dependencies: Vec<String>,
}

impl KnownClassConstants {
    /// Resolves a class-constant name through declared parents, interfaces, and used traits.
    /// Returns `None` when a dependency is unknown or cyclic, so folding stays conservative.
    fn constant_exists(&self, class_name: &str, constant_name: &str) -> Option<bool> {
        self.constant_exists_inner(
            &php_symbol_key(class_name.trim_start_matches('\\')),
            constant_name,
            &mut HashSet::new(),
        )
    }

    /// Performs cycle-safe recursive lookup for [`Self::constant_exists`].
    fn constant_exists_inner(
        &self,
        class_key: &str,
        constant_name: &str,
        visiting: &mut HashSet<String>,
    ) -> Option<bool> {
        if !visiting.insert(class_key.to_string()) {
            return None;
        }
        let declaration = self.declarations.get(class_key)?;
        if declaration.scoped.contains(constant_name) {
            visiting.remove(class_key);
            return None;
        }
        if declaration.constants.contains(constant_name) {
            visiting.remove(class_key);
            return Some(true);
        }
        for dependency in &declaration.dependencies {
            match self.constant_exists_inner(dependency, constant_name, visiting) {
                Some(true) => {
                    visiting.remove(class_key);
                    return Some(true);
                }
                Some(false) => {}
                None => {
                    visiting.remove(class_key);
                    return None;
                }
            }
        }
        visiting.remove(class_key);
        Some(false)
    }
}

thread_local! {
    static ACTIVE_KNOWN_CLASS_CONSTANTS: RefCell<Option<KnownClassConstants>> = const { RefCell::new(None) };
}

/// Installs class-constant facts collected from unconditional top-level declarations while
/// folding the supplied program, then restores any enclosing fold context.
pub(in crate::optimize) fn with_known_class_constants<R>(
    program: Program,
    f: impl FnOnce(Program) -> R,
) -> R {
    let facts = collect_known_class_constants(&program);
    ACTIVE_KNOWN_CLASS_CONSTANTS.with(|slot| {
        let previous = slot.replace(Some(facts));
        let result = f(program);
        slot.replace(previous);
        result
    })
}

/// Returns a boolean literal for a provable literal class-constant probe, or `None` when the
/// call name, argument shape, class, inheritance, or trait composition keeps the answer open.
pub(in crate::optimize) fn fold_known_class_constant_defined(
    name: &Name,
    args: &[Expr],
) -> Option<ExprKind> {
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "defined" || args.len() != 1 {
        return None;
    }
    let ExprKind::StringLiteral(probe) = &args[0].kind else {
        return None;
    };
    let (class_name, constant_name) = probe.trim_start_matches('\\').split_once("::")?;
    if class_name.is_empty() || constant_name.is_empty() {
        return None;
    }

    ACTIVE_KNOWN_CLASS_CONSTANTS.with(|slot| {
        let facts = slot.borrow();
        facts
            .as_ref()?
            .constant_exists(class_name, constant_name)
            .map(ExprKind::BoolLiteral)
    })
}

/// Collects only declarations whose placement is unconditionally visible to the merged AST.
fn collect_known_class_constants(program: &Program) -> KnownClassConstants {
    let mut facts = KnownClassConstants::default();
    for stmt in program {
        collect_top_level_declaration(stmt, &mut facts);
    }
    facts
}

/// Records one top-level class-like declaration and deliberately ignores conditional bodies.
fn collect_top_level_declaration(stmt: &Stmt, facts: &mut KnownClassConstants) {
    let (name, constants, dependencies) = match &stmt.kind {
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            trait_uses,
            constants,
            ..
        } => {
            let dependencies = extends
                .iter()
                .chain(implements)
                .map(|dependency| php_symbol_key(dependency.as_str()))
                .chain(trait_uses.iter().flat_map(|usage| {
                    usage
                        .trait_names
                        .iter()
                        .map(|dependency| php_symbol_key(dependency.as_str()))
                }))
                .collect();
            (name, constants.as_slice(), dependencies)
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            constants,
            ..
        } => (
            name,
            constants.as_slice(),
            extends
                .iter()
                .map(|dependency| php_symbol_key(dependency.as_str()))
                .collect(),
        ),
        StmtKind::EnumDecl {
            name,
            cases,
            implements,
            trait_uses,
            constants,
            ..
        } => {
            let dependencies = implements
                .iter()
                .map(|dependency| php_symbol_key(dependency.as_str()))
                .chain(trait_uses.iter().flat_map(|usage| {
                    usage
                        .trait_names
                        .iter()
                        .map(|dependency| php_symbol_key(dependency.as_str()))
                }))
                .collect();
            let mut declaration = split_by_visibility(constants, dependencies);
            declaration
                .constants
                .extend(cases.iter().map(|case| case.name.clone()));
            facts
                .declarations
                .insert(php_symbol_key(name.trim_start_matches('\\')), declaration);
            return;
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => {
            for nested in body {
                collect_top_level_declaration(nested, facts);
            }
            return;
        }
        _ => return,
    };

    facts.declarations.insert(
        php_symbol_key(name.trim_start_matches('\\')),
        split_by_visibility(constants, dependencies),
    );
}

/// Sorts a declaration's constants into foldable public names and scope-dependent ones.
fn split_by_visibility(
    constants: &[crate::parser::ast::ClassConst],
    dependencies: Vec<String>,
) -> KnownClassDeclaration {
    let mut declaration = KnownClassDeclaration {
        dependencies,
        ..KnownClassDeclaration::default()
    };
    for constant in constants {
        if matches!(constant.visibility, crate::parser::ast::Visibility::Public) {
            declaration.constants.insert(constant.name.clone());
        } else {
            declaration.scoped.insert(constant.name.clone());
        }
    }
    declaration
}
