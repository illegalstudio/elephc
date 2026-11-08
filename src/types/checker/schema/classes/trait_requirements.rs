//! Purpose:
//! Records, per class, the methods its used traits declare abstract, so the class schema can
//! treat them as PHP does: requirements on the class, not declarations of it.
//!
//! Called from:
//! - `crate::types::checker::driver` right after trait flattening, next to
//!   `collect_trait_default_origins`.
//! - `super::methods` while class methods are applied to the schema.
//!
//! Key details:
//! - Flattening merges a trait's abstract method into the class like any other member, so the
//!   schema saw `abstract function invalidate()` as written on the class: an inherited
//!   implementation then read as a re-abstraction ("Cannot make concrete method abstract"), or as
//!   an override of a final method, and `#[\Override]` on the method that satisfies the
//!   requirement found no target.
//! - Requirements are collected through nested trait uses, by trait name, case-insensitively.
//! - Methods the class body declares itself are recorded too: a requirement satisfied by an
//!   inherited method is skipped only when the abstract method is the trait's, never when the
//!   class itself re-declares the method abstract.
//! - A skipped requirement still checks the inherited signature against it
//!   (`validate_inherited_implementation`), with the rules enums apply to trait requirements.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, Stmt, StmtKind, TraitUse};
use crate::types::traits::FlattenedClass;
use crate::types::{FunctionSig, PhpType};

use super::super::super::Checker;
use super::super::class_constants::strict_type_accepts;
use super::super::validation::{build_method_sig, validate_abstract_trait_signature};

/// The trait requirements of one class.
#[derive(Debug, Clone, Default)]
pub(crate) struct TraitMethodRequirements {
    /// Method keys (`php_symbol_key`) declared abstract by a used trait, directly or nested.
    pub required: HashSet<String>,
    /// Method keys the class body declares itself.
    pub declared: HashSet<String>,
}

impl TraitMethodRequirements {
    /// Whether `key` is a trait requirement the class body does not declare itself, so the
    /// flattened abstract method is the trait's.
    pub(crate) fn is_imported_requirement(&self, key: &str) -> bool {
        self.required.contains(key) && !self.declared.contains(key)
    }
}

/// One trait declaration's abstract method keys and the traits it uses itself.
struct TraitShape {
    abstract_methods: HashSet<String>,
    uses: Vec<String>,
}

/// Collects the trait requirements of every class that uses a trait, keyed by class name.
pub(crate) fn collect_trait_method_requirements(
    program: &Program,
    classes: &[FlattenedClass],
) -> HashMap<String, TraitMethodRequirements> {
    let mut traits = HashMap::new();
    collect_trait_shapes(program, &mut traits);
    let mut requirements = HashMap::new();
    collect_class_requirements(program, classes, &traits, &mut requirements);
    requirements
}

/// Indexes every trait declaration, including those nested in namespace blocks.
fn collect_trait_shapes(program: &[Stmt], traits: &mut HashMap<String, TraitShape>) {
    for statement in program {
        match &statement.kind {
            StmtKind::TraitDecl { name, trait_uses, methods, .. } => {
                traits.insert(php_symbol_key(name), TraitShape {
                    abstract_methods: abstract_method_keys(methods),
                    uses: used_trait_keys(trait_uses),
                });
            }
            StmtKind::NamespaceBlock { body, .. } => collect_trait_shapes(body, traits),
            _ => {}
        }
    }
}

/// Records the requirements of each class declaration that uses at least one trait.
fn collect_class_requirements(
    program: &[Stmt],
    classes: &[FlattenedClass],
    traits: &HashMap<String, TraitShape>,
    requirements: &mut HashMap<String, TraitMethodRequirements>,
) {
    for statement in program {
        match &statement.kind {
            StmtKind::ClassDecl { name, trait_uses, methods, .. } => {
                if trait_uses.is_empty() || !classes.iter().any(|class| class.name == *name) {
                    continue;
                }
                let mut required = HashSet::new();
                let mut visited = HashSet::new();
                for used in used_trait_keys(trait_uses) {
                    gather_required(&used, traits, &mut visited, &mut required);
                }
                if required.is_empty() {
                    continue;
                }
                let declared = methods.iter().map(|method| php_symbol_key(&method.name)).collect();
                requirements.insert(name.clone(), TraitMethodRequirements { required, declared });
            }
            StmtKind::NamespaceBlock { body, .. } => {
                collect_class_requirements(body, classes, traits, requirements);
            }
            _ => {}
        }
    }
}

/// Adds `trait_key`'s abstract methods and those of the traits it uses, once per trait.
fn gather_required(
    trait_key: &str,
    traits: &HashMap<String, TraitShape>,
    visited: &mut HashSet<String>,
    required: &mut HashSet<String>,
) {
    if !visited.insert(trait_key.to_string()) {
        return;
    }
    let Some(shape) = traits.get(trait_key) else { return };
    required.extend(shape.abstract_methods.iter().cloned());
    for nested in &shape.uses {
        gather_required(nested, traits, visited, required);
    }
}

/// Returns the keys of the abstract methods among `methods`.
fn abstract_method_keys(methods: &[ClassMethod]) -> HashSet<String> {
    methods
        .iter()
        .filter(|method| method.is_abstract)
        .map(|method| php_symbol_key(&method.name))
        .collect()
}

/// Returns the lookup keys of every trait named by `trait_uses`.
fn used_trait_keys(trait_uses: &[TraitUse]) -> Vec<String> {
    trait_uses
        .iter()
        .flat_map(|use_decl| use_decl.trait_names.iter().map(|name| php_symbol_key(name.as_str())))
        .collect()
}

/// Checks that an inherited implementation is compatible with the trait requirement it
/// satisfies: parameters may only widen and a declared return type must be honoured, as PHP
/// reports `Declaration of P::foo() must be compatible with T::foo()` otherwise.
pub(crate) fn validate_inherited_implementation(
    checker: &Checker,
    class: &FlattenedClass,
    required: &ClassMethod,
    inherited: &FunctionSig,
) -> Result<(), CompileError> {
    let mut required = required.clone();
    required.substitute_relative_class_types(&class.name, None);
    let required_sig = build_method_sig(checker, &required, &class.name)?;
    validate_abstract_trait_signature(
        checker, required.span, &class.name, &required.name, inherited, &required_sig,
    )?;
    let late_static = required.return_type.as_ref().is_some_and(|hint| hint.contains_late_static());
    if required_sig.declared_return
        && !late_static
        && !matches!(inherited.return_type, PhpType::Never)
        && (!inherited.declared_return
            || !strict_type_accepts(checker, &required_sig.return_type, &inherited.return_type, false))
    {
        return Err(CompileError::new(required.span, &format!(
            "Cannot change return type when implementing trait method: {}::{}",
            class.name, required.name,
        )));
    }
    Ok(())
}
