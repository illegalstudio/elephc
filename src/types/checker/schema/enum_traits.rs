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
//! - A requirement a trait implements itself, by its own method or another used trait's, is
//!   checked against that method and does not reach the trait's users or their `as` aliases:
//!   a class may replace or alias the trait's concrete method freely, as in PHP.
//! - Each trait's contract is memoized once complete (`RequirementWalk`), and copies of one
//!   method reached along several `use` paths are kept once, keyed by the declaring trait
//!   instantiation, so traits shared along many paths are not walked again per path.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Program, StmtKind, TraitAdaptation, TraitUse, TypeExpr};
use crate::span::Span;
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
        let contract = RequirementWalk::new(&traits).uses_contract(uses);
        for (required, actual) in &contract.satisfied {
            validate_requirement(checker, enum_unit, &required.method, &actual.method)?;
        }
        for required in contract.required.into_iter().map(|required| required.method) {
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
/// such method and is not checked here. `reported` holds the errors already produced, where a
/// used trait's own incompatibility replaces the error of a class with an abstract trait
/// binding failure (the failure, or the build error `class_build_errors` locates).
pub(crate) fn validate_class_trait_requirements(
    checker: &Checker,
    program: &Program,
    classes: &[FlattenedClass],
    reported: &mut Vec<CompileError>,
    class_build_errors: &HashMap<String, usize>,
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
    // A pair a trait satisfied itself is one declaration, checked once by its first user, unless
    // it names `self`, `parent` or `static`: those resolve against each consuming class, so the
    // pair is checked for every user and the outcome does not depend on declaration order.
    let mut checked_pairs = HashSet::new();
    // Recorded pairs that failed, so a later class reports the same trait error only once.
    let mut failed_pairs = HashSet::new();
    // Edits to `reported`, applied once every class is walked so the indices stay valid.
    let mut replacements: Vec<(usize, CompileError)> = Vec::new();
    let mut removals: Vec<usize> = Vec::new();
    for class in classes.iter().filter(|class| !class.used_traits.is_empty()) {
        let Some(uses) = class_uses.get(class.name.as_str()) else { continue };
        let contract = walk.uses_contract(uses);
        // A class with an abstract trait binding failure checked the requirements bound before
        // it while it was built, and PHP never binds the ones after it. A pair a used trait
        // satisfied itself is still checked: PHP compiles that trait before it binds the class,
        // so its error takes the place of the class's binding failure.
        let binding_failure =
            checker.final_abstract_binding_failures.get(&php_symbol_key(&class.name));

        // The error the class schema reported for a class with a binding failure: the failure
        // itself, or an inheritance error PHP reports before it, either one after the used
        // traits are compiled.
        let class_error = binding_failure.and_then(|failure| {
            reported
                .iter()
                .position(|reported| {
                    reported.span == failure.span && reported.message == failure.message
                })
                .or_else(|| class_build_errors.get(&php_symbol_key(&class.name)).copied())
        });
        // A class whose build error is its parent's (`class_build_errors` has no entry) reports
        // that parent's error, as PHP does before it compiles the class; its used traits add
        // nothing.
        if binding_failure.is_some() && class_error.is_none() {
            continue;
        }
        let mut pairs: Vec<(&ClassMethod, &ClassMethod, Option<RequirementPairKey>)> = Vec::new();
        for (required, actual) in &contract.satisfied {
            let key = (required.identity(), actual.identity());
            // Only pairs free of relative types are recorded, so a recorded pair is skipped.
            if checked_pairs.contains(&key) {
                if failed_pairs.contains(&key) {
                    removals.extend(class_error);
                }
                continue;
            }
            let recorded = !mentions_relative_class_types(&required.method)
                && !mentions_relative_class_types(&actual.method);
            if recorded {
                checked_pairs.insert(key.clone());
            }
            pairs.push((&required.method, &actual.method, recorded.then_some(key)));
        }
        for required in contract.required.iter().filter(|_| binding_failure.is_none()) {
            if let Some(actual) = class.methods.iter().find(|method| {
                !method.is_abstract && method.is_static == required.method.is_static
                    && php_symbol_key(&method.name) == php_symbol_key(&required.method.name)
            }) {
                pairs.push((&required.method, actual, None));
            }
        }
        for (required, actual, key) in pairs {
            if let Err(error) = validate_class_requirement(checker, class, required, actual) {
                if let Some(key) = key {
                    failed_pairs.insert(key);
                }
                match class_error {
                    Some(index) => replacements.push((index, error)),
                    None => errors.extend(error.flatten()),
                }
                break;
            }
        }
    }
    apply_class_error_edits(reported, replacements, removals);
    errors
}

/// Applies the class error edits of `validate_class_trait_requirements` to `reported`: each
/// replacement puts a used trait's own requirement incompatibility in place of a class's error
/// (PHP compiles that trait before it binds the class), and each removal drops a class's error
/// whose trait incompatibility an earlier class already reported. Two classes can share one
/// error (their common parent's), so a slot that holds a replacement is never removed.
fn apply_class_error_edits(
    reported: &mut Vec<CompileError>,
    replacements: Vec<(usize, CompileError)>,
    mut removals: Vec<usize>,
) {
    removals.retain(|index| !replacements.iter().any(|(replaced, _)| replaced == index));
    for (index, error) in replacements {
        if let Some(slot) = reported.get_mut(index) {
            *slot = error;
        }
    }
    removals.sort_unstable();
    removals.dedup();
    for index in removals.into_iter().rev() {
        if index < reported.len() {
            reported.remove(index);
        }
    }
}

/// The identity of one trait method (`DeclaredMethod::identity`).
type RequirementIdentity = (String, Span, String, bool, String);

/// The identity of a required trait method and the method that satisfies it.
type RequirementPairKey = (RequirementIdentity, RequirementIdentity);

/// Whether `method`'s parameter, variadic or return types name `self`, `parent` or `static`,
/// which resolve against the class that consumes the trait.
fn mentions_relative_class_types(method: &ClassMethod) -> bool {
    method
        .params
        .iter()
        .filter_map(|(_, type_ann, _, _)| type_ann.as_ref())
        .chain(method.variadic_type.as_ref())
        .chain(method.return_type.as_ref())
        .any(type_mentions_relative_class)
}

/// Whether `hint` names `self`, `parent` or `static` anywhere, walking the same variants
/// `TypeExpr::substitute_relative_class_types` rewrites, by reference.
fn type_mentions_relative_class(hint: &TypeExpr) -> bool {
    match hint {
        TypeExpr::Named(name) => ["self", "static", "parent"]
            .iter()
            .any(|relative| name.as_str().eq_ignore_ascii_case(relative)),
        TypeExpr::Nullable(inner) | TypeExpr::Array(inner) | TypeExpr::Buffer(inner) => {
            type_mentions_relative_class(inner)
        }
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            members.iter().any(type_mentions_relative_class)
        }
        TypeExpr::GenericClass { args, .. } => args.iter().any(type_mentions_relative_class),
        TypeExpr::CallableSig { params, ret } => {
            params.iter().any(type_mentions_relative_class) || type_mentions_relative_class(ret)
        }
        _ => false,
    }
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
pub(in crate::types::checker::schema) fn validate_class_requirement(
    checker: &Checker,
    class: &FlattenedClass,
    required: &ClassMethod,
    actual: &ClassMethod,
) -> Result<(), CompileError> {
    let mut required = required.clone();
    required.substitute_relative_class_types(&class.name, class.extends.as_deref());
    let required_sig = build_method_sig(checker, &required, &class.name)?;
    // A method a trait satisfied a requirement with is still the trait's raw declaration, whose
    // `self`/`parent` types name the consuming class once flattened.
    let mut actual = actual.clone();
    actual.substitute_relative_class_types(&class.name, class.extends.as_deref());
    let actual = &actual;
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

/// A trait method with the trait that declares it. `origin` names the instantiated trait
/// (`Needs<int>`, `Needs<string>`), because a generic trait's instantiations share the
/// template's declaration spans.
#[derive(Clone)]
struct DeclaredMethod {
    origin: String,
    method: ClassMethod,
}

impl DeclaredMethod {
    /// The identity of one copy of a method: its declaring trait, declaration, name, kind and
    /// visibility. Copies reached along several `use` paths share it.
    fn identity(&self) -> (String, Span, String, bool, String) {
        (
            self.origin.clone(),
            self.method.span,
            php_symbol_key(&self.method.name),
            self.method.is_static,
            format!("{:?}", self.method.visibility),
        )
    }

    /// Whether `self` is a method that satisfies the abstract `required` one: same name and kind.
    fn satisfies(&self, required: &ClassMethod) -> bool {
        self.method.is_static == required.is_static
            && php_symbol_key(&self.method.name) == php_symbol_key(&required.name)
    }
}

/// What a trait (or one `use` list) brings into its user once flattened.
#[derive(Clone, Default)]
struct TraitContract {
    /// Abstract methods still required of the user, with `as` adaptations applied.
    required: Vec<DeclaredMethod>,
    /// Concrete methods the user receives, aliases included, after `insteadof` exclusions.
    provided: Vec<DeclaredMethod>,
    /// Requirements a trait satisfied itself, with the method that satisfies each: PHP checks
    /// that pair where the trait is composed (`Declaration of A::foo() must be compatible with
    /// N::foo()`), and the requirement does not reach the trait's users or their aliases.
    satisfied: Vec<(DeclaredMethod, DeclaredMethod)>,
}

impl TraitContract {
    /// Drops the copies of one method reached along several `use` paths, so a diamond of shared
    /// traits does not grow each list exponentially.
    fn dedup(&mut self) {
        let mut seen = HashSet::new();
        self.required.retain(|method| seen.insert(method.identity()));
        let mut seen = HashSet::new();
        self.provided.retain(|method| seen.insert(method.identity()));
        let mut seen = HashSet::new();
        self.satisfied.retain(|(required, actual)| seen.insert((required.identity(), actual.identity())));
    }
}

/// The trait index plus the memo of each trait's completed contract, shared by every use list
/// walked for one program: a trait reached along several `use` paths is walked once, so a
/// diamond of shared nested traits stays linear instead of exponential, as in
/// `trait_requirements::RequirementWalk`.
struct RequirementWalk<'t, 'a> {
    traits: &'t HashMap<String, TraitMembers<'a>>,
    /// Completed contracts by trait key.
    done: HashMap<String, TraitContract>,
    /// Traits on the current path, only to break use cycles (flattening reports those).
    visiting: HashSet<String>,
}

impl<'t, 'a> RequirementWalk<'t, 'a> {
    /// Creates a walk over `traits` with an empty memo and no trait being visited.
    fn new(traits: &'t HashMap<String, TraitMembers<'a>>) -> Self {
        Self { traits, done: HashMap::new(), visiting: HashSet::new() }
    }

    /// Returns what `trait_key` brings into a user. A requirement of a used trait that the
    /// trait implements, by its own method or another used trait's, moves to `satisfied`
    /// (`trait A { use N; function foo() {} }`), so a user may replace or alias `foo` freely.
    /// Each trait is computed once.
    fn trait_contract(&mut self, trait_key: &str) -> TraitContract {
        if let Some(contract) = self.done.get(trait_key) {
            return contract.clone();
        }
        let traits = self.traits;
        let Some((nested, methods)) = traits.get(trait_key) else { return TraitContract::default() };
        if !self.visiting.insert(trait_key.to_string()) {
            return TraitContract::default();
        }
        let used = self.uses_contract(nested);
        let own = |abstract_methods: bool| {
            methods
                .iter()
                .filter(move |method| method.is_abstract == abstract_methods)
                .map(|method| DeclaredMethod { origin: trait_key.to_string(), method: method.clone() })
        };
        let own_concrete: Vec<DeclaredMethod> = own(false).collect();
        let declares = |name: &str| methods.iter().any(|method| php_symbol_key(&method.name) == php_symbol_key(name));
        let mut contract = TraitContract { satisfied: used.satisfied, ..TraitContract::default() };
        // The trait's own methods win over imported ones of the same name.
        contract.provided = own_concrete.clone();
        contract.provided.extend(used.provided.iter().filter(|method| !declares(&method.method.name)).cloned());
        for required in used.required {
            match contract.provided.iter().find(|method| method.satisfies(&required.method)) {
                Some(actual) => contract.satisfied.push((required, actual.clone())),
                None => contract.required.push(required),
            }
        }
        contract.required.extend(own(true));
        contract.dedup();
        self.visiting.remove(trait_key);
        self.done.insert(trait_key.to_string(), contract.clone());
        contract
    }

    /// Returns what `uses` bring into their user, preserving abstract requirements through each
    /// alias and visibility adaptation.
    fn uses_contract(&mut self, uses: &[TraitUse]) -> TraitContract {
        let mut contract = TraitContract::default();
        for usage in uses {
            // Each method is paired with the used trait it comes through, which is what an
            // adaptation's `Trait::` qualifier names.
            let mut required = Vec::new();
            let mut provided = Vec::new();
            for name in &usage.trait_names {
                let key = php_symbol_key(name.as_str());
                let used = self.trait_contract(&key);
                required.extend(used.required.into_iter().map(|method| (key.clone(), method)));
                provided.extend(used.provided.into_iter().map(|method| (key.clone(), method)));
                contract.satisfied.extend(used.satisfied);
            }
            // Aliases are taken from every candidate before `insteadof` exclusions apply, as
            // flattening does: `use P, Q { P::foo insteadof Q; Q::foo as bar; }` keeps `bar`.
            let mut aliases: [Vec<(String, DeclaredMethod)>; 2] = [Vec::new(), Vec::new()];
            for adaptation in &usage.adaptations {
                let TraitAdaptation::Alias { trait_name, method, alias, visibility, is_final } = adaptation else {
                    continue;
                };
                for (methods, aliases) in [&mut required, &mut provided].into_iter().zip(aliases.iter_mut()) {
                    for (via, declared) in methods.iter_mut() {
                        if php_symbol_key(&declared.method.name) != php_symbol_key(method)
                            || trait_name.as_ref().is_some_and(|name| php_symbol_key(name.as_str()) != *via)
                        { continue; }
                        if let Some(alias) = alias {
                            let mut adapted = declared.clone();
                            adapted.method.name = alias.clone();
                            if let Some(visibility) = visibility { adapted.method.visibility = visibility.clone(); }
                            adapted.method.is_final |= *is_final;
                            aliases.push((via.clone(), adapted));
                        } else {
                            if let Some(visibility) = visibility {
                                declared.method.visibility = visibility.clone();
                            }
                            declared.method.is_final |= *is_final;
                        }
                    }
                }
            }
            for adaptation in &usage.adaptations {
                // Selecting a concrete body does not discard another trait's abstract contract,
                // but the excluded traits' bodies are not received under their own name.
                let TraitAdaptation::InsteadOf { method, instead_of, .. } = adaptation else { continue };
                let excluded: HashSet<String> =
                    instead_of.iter().map(|name| php_symbol_key(name.as_str())).collect();
                provided.retain(|(via, declared): &(String, DeclaredMethod)| {
                    !excluded.contains(via) || php_symbol_key(&declared.method.name) != php_symbol_key(method)
                });
            }
            let [required_aliases, provided_aliases] = aliases;
            required.extend(required_aliases);
            provided.extend(provided_aliases);
            contract.required.extend(required.into_iter().map(|(_, method)| method));
            contract.provided.extend(provided.into_iter().map(|(_, method)| method));
        }
        contract.dedup();
        contract
    }
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
    // A method a trait satisfied a requirement with is still the trait's raw declaration.
    let mut actual = actual.clone();
    actual.substitute_relative_class_types(&enum_unit.name, None);
    let actual = &actual;
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
