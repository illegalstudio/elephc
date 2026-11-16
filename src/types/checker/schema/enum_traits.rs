//! Purpose:
//! Validates abstract trait method requirements against an enum's or a class's final
//! implementations.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()` after enum schema construction.
//!
//! Key details:
//! - Local methods win trait composition but must not erase abstract requirements.
//! - Nested trait requirements bind relative types to the consuming enum or class.
//! - For a class, flattening drops a requirement that another trait's method or the class's own
//!   method satisfies; that method must still be compatible with it, as PHP checks
//!   (`Declaration of C::foo() must be compatible with T::foo()`). Visibility is not compared:
//!   PHP 8.5 lets a private method satisfy an abstract public trait method.
//! - Each trait's adapted requirement list is memoized once complete (`RequirementWalk`), and
//!   copies of one abstract method reached along several `use` paths are kept once, so traits
//!   shared along many paths are not walked again per path.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, StmtKind, TraitAdaptation, TraitUse, TypeExpr};
use crate::types::{traits::FlattenedClass, PhpType};

use super::super::Checker;
use super::class_constants::strict_type_accepts;
use super::validation::{
    build_method_sig, late_static_return_compatible, validate_abstract_trait_signature, visibility_rank,
};

/// Raw requirements remain available even when trait flattening selects a concrete override.
type TraitMembers<'a> = (&'a [TraitUse], &'a [ClassMethod]);

/// Checks every recursively imported abstract contract against the enum's selected method.
pub(crate) fn validate_enum_trait_requirements(
    checker: &Checker,
    program: &Program,
    enum_unit: &FlattenedClass,
) -> Result<(), CompileError> {
    if enum_unit.used_traits.is_empty() {
        return Ok(());
    }
    let mut traits = HashMap::new();
    collect_trait_members(program, &mut traits);
    if let Some(uses) = enum_trait_uses(program, &enum_unit.name) {
        for required in RequirementWalk::new(&traits).adapted_requirements(uses) {
            if let Some(actual) = enum_unit.methods.iter().find(|method| {
                !method.is_abstract && method.is_static == required.is_static
                    && php_symbol_key(&method.name) == php_symbol_key(&required.name)
            }) {
                validate_requirement(checker, enum_unit, &required, actual)?;
            } else if required.is_static {
                if let Some(actual) = super::enum_factories::public_signature(checker, &enum_unit.name, &required.name) {
                    super::enum_factories::validate_trait_requirement(checker, &enum_unit.name, &required, &actual)?;
                }
            }
        }
    }
    Ok(())
}

/// Checks every recursively imported abstract trait method of each class against the concrete
/// method the class ended up with: its own method or another trait's. A requirement still
/// abstract after flattening (left to a subclass, or satisfied by an inherited method) has no
/// such method and is not checked here.
pub(crate) fn validate_class_trait_requirements(
    checker: &Checker,
    program: &Program,
    classes: &[FlattenedClass],
) -> Vec<CompileError> {
    let mut errors = Vec::new();
    if classes.iter().all(|class| class.used_traits.is_empty()) {
        return errors;
    }
    let mut traits = HashMap::new();
    collect_trait_members(program, &mut traits);
    let mut class_uses = HashMap::new();
    collect_class_trait_uses(program, &mut class_uses);
    let mut walk = RequirementWalk::new(&traits);
    for class in classes.iter().filter(|class| !class.used_traits.is_empty()) {
        let Some(uses) = class_uses.get(class.name.as_str()) else { continue };
        for required in walk.adapted_requirements(uses) {
            let Some(actual) = class.methods.iter().find(|method| {
                !method.is_abstract && method.is_static == required.is_static
                    && php_symbol_key(&method.name) == php_symbol_key(&required.name)
            }) else { continue };
            if let Err(error) = validate_class_requirement(checker, class, &required, actual) {
                errors.extend(error.flatten());
                break;
            }
        }
    }
    errors
}

/// Indexes each class declaration's trait uses by class name, through namespace blocks.
fn collect_class_trait_uses<'a>(program: &'a [crate::parser::ast::Stmt], uses: &mut HashMap<&'a str, &'a [TraitUse]>) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::ClassDecl { name, trait_uses, .. } if !trait_uses.is_empty() => {
                uses.insert(name.as_str(), trait_uses.as_slice());
            }
            StmtKind::NamespaceBlock { body, .. } => collect_class_trait_uses(body, uses),
            _ => {}
        }
    }
}

/// Applies PHP's prototype check to the method that satisfies one abstract trait method: the
/// parameters may only widen, a by-reference return stays by reference, and a declared return
/// type must be honoured. Visibility is not compared (see the module notes).
fn validate_class_requirement(
    checker: &Checker,
    class: &FlattenedClass,
    required: &ClassMethod,
    actual: &ClassMethod,
) -> Result<(), CompileError> {
    let mut required = required.clone();
    required.substitute_relative_class_types(&class.name, class.extends.as_deref());
    let required_sig = build_method_sig(checker, &required, &class.name)?;
    let actual_sig = build_method_sig(checker, actual, &class.name)?;
    validate_abstract_trait_signature(
        checker, actual.span, &class.name, &actual.name, &actual_sig, &required_sig,
    )?;
    if required_sig.by_ref_return && !actual_sig.by_ref_return {
        return Err(CompileError::new(actual.span, &format!(
            "Cannot remove by-reference return when implementing trait method: {}::{}",
            class.name, actual.name,
        )));
    }
    let late_static_compatible = late_static_return_compatible(
        checker, required.return_type.as_ref().filter(|hint| hint.contains_late_static()),
        actual.return_type.as_ref(), &actual_sig.return_type, &class.name, actual.span,
    )?;
    if required_sig.declared_return
        && !matches!(actual_sig.return_type, PhpType::Never)
        && (!actual_sig.declared_return
            || declares_void_return(&required) != declares_void_return(actual)
            || !late_static_compatible.unwrap_or_else(|| {
                strict_type_accepts(checker, &required_sig.return_type, &actual_sig.return_type, false)
            }))
    {
        return Err(CompileError::new(actual.span, &format!(
            "Cannot change return type when implementing trait method: {}::{}",
            class.name, actual.name,
        )));
    }
    Ok(())
}

/// Whether a method declares `void`. `PhpType::Void` stands for both `void` and `null`, so a
/// return check compares this declared syntax first: a `void` method satisfies only a `void`
/// requirement, and a `void` requirement only a `void` or `never` method
/// (`abstract function foo(): ?int;` rejects `function foo(): void {}`, as PHP does).
fn declares_void_return(method: &ClassMethod) -> bool {
    matches!(method.return_type, Some(TypeExpr::Void))
}

/// Finds the consuming enum's source adaptations, including namespace-only checker fixtures.
fn enum_trait_uses<'a>(program: &'a Program, name: &str) -> Option<&'a [TraitUse]> {
    program.iter().find_map(|stmt| match &stmt.kind {
        StmtKind::EnumDecl { name: candidate, trait_uses, .. } if candidate == name => {
            Some(trait_uses.as_slice())
        }
        StmtKind::NamespaceBlock { body, .. } => enum_trait_uses(body, name),
        _ => None,
    })
}

/// The trait index plus the memo of each trait's completed requirement list, shared by every
/// use list walked for one program: a trait reached along several `use` paths is walked once,
/// so a diamond of shared nested traits stays linear instead of exponential, as in
/// `trait_requirements::RequirementWalk`.
struct RequirementWalk<'t, 'a> {
    traits: &'t HashMap<String, TraitMembers<'a>>,
    /// Completed requirement lists by trait key, aliases and visibility adaptations applied.
    done: HashMap<String, Vec<ClassMethod>>,
    /// Traits on the current path, only to break use cycles (flattening reports those).
    visiting: HashSet<String>,
}

impl<'t, 'a> RequirementWalk<'t, 'a> {
    fn new(traits: &'t HashMap<String, TraitMembers<'a>>) -> Self {
        Self { traits, done: HashMap::new(), visiting: HashSet::new() }
    }

    /// Returns the abstract methods `trait_key` brings into a user: those of the traits it
    /// uses, adapted by its own `use` blocks, then its own. Each trait is computed once.
    fn trait_requirements(&mut self, trait_key: &str) -> Vec<ClassMethod> {
        if let Some(requirements) = self.done.get(trait_key) {
            return requirements.clone();
        }
        let traits = self.traits;
        let Some((nested, methods)) = traits.get(trait_key) else { return Vec::new() };
        if !self.visiting.insert(trait_key.to_string()) {
            return Vec::new();
        }
        let mut members = self.adapted_requirements(nested);
        members.extend(methods.iter().filter(|method| method.is_abstract).cloned());
        dedup_requirements(&mut members);
        self.visiting.remove(trait_key);
        self.done.insert(trait_key.to_string(), members.clone());
        members
    }

    /// Preserves abstract requirements through each nested trait alias and visibility adaptation.
    fn adapted_requirements(&mut self, uses: &[TraitUse]) -> Vec<ClassMethod> {
        let mut requirements = Vec::new();
        for usage in uses {
            let mut imported = Vec::new();
            for name in &usage.trait_names {
                let key = php_symbol_key(name.as_str());
                let members = self.trait_requirements(&key);
                imported.extend(members.into_iter().map(|method| (key.clone(), method)));
            }
            for adaptation in &usage.adaptations {
                match adaptation {
                    // Selecting a concrete body does not discard another trait's abstract contract.
                    TraitAdaptation::InsteadOf { .. } => {}
                    TraitAdaptation::Alias { trait_name, method, alias, visibility, .. } => {
                        let mut aliases = Vec::new();
                        for (origin, required) in &mut imported {
                            if php_symbol_key(&required.name) != php_symbol_key(method)
                                || trait_name.as_ref().is_some_and(|name| php_symbol_key(name.as_str()) != *origin)
                            { continue; }
                            if let Some(alias) = alias {
                                let mut adapted = required.clone();
                                adapted.name = alias.clone();
                                if let Some(visibility) = visibility { adapted.visibility = visibility.clone(); }
                                aliases.push((origin.clone(), adapted));
                            } else if let Some(visibility) = visibility {
                                required.visibility = visibility.clone();
                            }
                        }
                        imported.extend(aliases);
                    }
                }
            }
            requirements.extend(imported.into_iter().map(|(_, method)| method));
        }
        dedup_requirements(&mut requirements);
        requirements
    }
}

/// Drops the copies of one abstract method reached along several `use` paths: same declaration
/// (span), name, kind and visibility. Each copy would be checked against the same method.
fn dedup_requirements(requirements: &mut Vec<ClassMethod>) {
    let mut seen = HashSet::new();
    requirements.retain(|method| {
        seen.insert((
            method.span,
            php_symbol_key(&method.name),
            method.is_static,
            format!("{:?}", method.visibility),
        ))
    });
}

/// Collects raw trait declarations, including namespace blocks used by checker-only fixtures.
fn collect_trait_members<'a>(program: &'a Program, traits: &mut HashMap<String, TraitMembers<'a>>) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, trait_uses, methods, .. } => {
                traits.insert(php_symbol_key(name), (trait_uses, methods));
            }
            StmtKind::NamespaceBlock { body, .. } => collect_trait_members(body, traits),
            _ => {}
        }
    }
}

/// Applies shared override rules and non-coercive parameter variance to one abstract requirement.
fn validate_requirement(
    checker: &Checker,
    enum_unit: &FlattenedClass,
    required: &ClassMethod,
    actual: &ClassMethod,
) -> Result<(), CompileError> {
    let mut required = required.clone();
    required.substitute_relative_class_types(&enum_unit.name, None);
    let required_sig = build_method_sig(checker, &required, &enum_unit.name)?;
    let actual_sig = build_method_sig(checker, actual, &enum_unit.name)?;
    validate_abstract_trait_signature(
        checker, actual.span, &enum_unit.name, &actual.name, &actual_sig, &required_sig,
    )?;
    let late_static_compatible = late_static_return_compatible(
        checker, required.return_type.as_ref().filter(|hint| hint.contains_late_static()),
        actual.return_type.as_ref(), &actual_sig.return_type, &enum_unit.name, actual.span,
    )?;
    if visibility_rank(&actual.visibility) < visibility_rank(&required.visibility) {
        return Err(CompileError::new(actual.span, &format!(
            "Cannot reduce visibility when implementing trait method: {}::{}",
            enum_unit.name, actual.name,
        )));
    }
    if required_sig.by_ref_return && !actual_sig.by_ref_return {
        return Err(CompileError::new(actual.span, &format!(
            "Cannot remove by-reference return when implementing trait method: {}::{}",
            enum_unit.name, actual.name,
        )));
    }
    if required_sig.declared_return
        && !matches!(actual_sig.return_type, PhpType::Never)
        && (!actual_sig.declared_return
            || declares_void_return(&required) != declares_void_return(actual)
            || !late_static_compatible.unwrap_or_else(|| {
                strict_type_accepts(checker, &required_sig.return_type, &actual_sig.return_type, false)
            }))
    {
        return Err(CompileError::new(actual.span, &format!(
            "Enum method {}::{} has incompatible return type for abstract trait method",
            enum_unit.name, actual.name,
        )));
    }
    Ok(())
}
