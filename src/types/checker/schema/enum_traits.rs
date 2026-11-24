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
//! - PHP checks a requirement a trait satisfied itself when it declares that trait, so those
//!   pairs are checked in trait declaration order (not `use` order), once for every class and
//!   enum, and a trait no walked user checks (unused, or used only by an enum that failed to
//!   build) is checked on its own. A method of the other kind (static or not) cannot satisfy a
//!   requirement, and PHP rejects the trait that pairs them.
//! - Each error carries the `LinkOrder` PHP reports it in: the trait's declaration for a pair it
//!   satisfied, the class or enum otherwise. The driver sorts every linking error by it.

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

/// Raw requirements remain available even when trait flattening selects a concrete override:
/// the trait's declared name, its `use` lists and its methods.
type TraitMembers<'a> = (&'a str, &'a [TraitUse], &'a [ClassMethod]);

/// When PHP reports an error it raises while linking declarations: an early-bound class links
/// while PHP compiles the file (`false`), before every declaration PHP links when it runs the
/// file (`true`), and each group follows source order (`declaration_positions`). Sorting the
/// linking errors by it puts first the error PHP reports.
pub(crate) type LinkOrder = (bool, usize);

/// What the requirement walk has done with the pairs traits satisfied themselves.
#[derive(Default)]
struct PairChecks {
    /// Pairs checked once for all users: those free of `self`, `parent` and `static`, which
    /// resolve against each consuming class (the pair is then checked for every user).
    checked: HashSet<RequirementPairKey>,
    /// Checked pairs that failed, so a later class reports the same trait error only once.
    failed: HashSet<RequirementPairKey>,
    /// Traits with a failed pair: PHP stops declaring a trait at its first incompatibility.
    failed_traits: HashSet<String>,
    /// Pairs of a class whose parent failed while PHP compiled the file, which PHP never reaches.
    suppressed: HashSet<RequirementPairKey>,
}

/// Checks the abstract trait methods every class and enum imports, and adds each error to
/// `linked` with the order PHP reports it in.
///
/// A requirement a class or enum method satisfies is checked at that class or enum. A
/// requirement still abstract after flattening (left to a subclass, or satisfied by an
/// inherited method) has no such method and is not checked here. A requirement a trait
/// satisfied itself is checked by the first class or enum that uses the trait, in trait
/// declaration order, and its error is ordered at that trait; a trait no walked user checks
/// (unused, or used only by an enum that failed to build) is checked on its own, named after
/// itself. A class with an abstract trait binding failure has an error slot in `linked` (the
/// failure, or the build error `class_build_errors` locates), which a used trait's own
/// incompatibility takes when PHP declares that trait first.
pub(crate) fn validate_trait_requirements(
    checker: &Checker,
    program: &Program,
    classes: &[FlattenedClass],
    enums: &[&FlattenedClass],
    linked: &mut Vec<(LinkOrder, CompileError)>,
    class_build_errors: &HashMap<String, usize>,
) {
    let mut traits = HashMap::new();
    collect_trait_members(program, &mut traits);
    if traits.is_empty() {
        return;
    }
    let positions = declaration_positions(program);
    let mut class_uses = HashMap::new();
    collect_class_trait_uses(program, &mut class_uses);
    let mut walk = RequirementWalk::new(&traits);
    let mut checks = PairChecks::default();
    let mut errors: Vec<(LinkOrder, CompileError)> = Vec::new();
    // Edits to `linked`, applied once every class is walked so the indices stay valid.
    let mut replacements: Vec<(usize, LinkOrder, CompileError)> = Vec::new();
    let mut removals: Vec<usize> = Vec::new();
    for class in classes.iter().filter(|class| !class.used_traits.is_empty()) {
        let Some(uses) = class_uses.get(class.name.as_str()) else { continue };
        let contract = walk.uses_contract(uses);
        // A class with an abstract trait binding failure checked the requirements bound before
        // it while it was built, and PHP never binds the ones after it. A pair a used trait
        // satisfied itself is still checked: PHP declares that trait before it binds the class,
        // so its error takes the place of the class's binding failure.
        let binding_failure =
            checker.final_abstract_binding_failures.get(&php_symbol_key(&class.name));

        // The error the class schema reported for a class with a binding failure: the failure
        // itself, or an inheritance error PHP reports before it, either one after the used
        // traits are declared.
        let class_error = binding_failure.and_then(|failure| {
            linked
                .iter()
                .position(|(_, reported)| {
                    reported.span == failure.span && reported.message == failure.message
                })
                .or_else(|| class_build_errors.get(&php_symbol_key(&class.name)).copied())
        });
        // A class whose build error is its parent's compile error (`class_build_errors` has no
        // entry) reports that error, and PHP stops before it declares any trait.
        if binding_failure.is_some() && class_error.is_none() {
            checks.suppressed.extend(contract.satisfied.iter().map(SatisfiedPair::key));
            continue;
        }
        // The class's error slot, when PHP reports the trait declared at `position` first.
        let slot_after = |position: usize| {
            class_error.filter(|index| (true, position) < linked[*index].0)
        };
        let mut failure = None;
        for pair in in_declaration_order(&contract.satisfied, &positions) {
            let composer = position_of(&positions, &pair.composer);
            let key = pair.key();
            if checks.checked.contains(&key) {
                if checks.failed.contains(&key) {
                    removals.extend(slot_after(composer));
                }
                continue;
            }
            if let Err(error) = checks.check(pair, &traits, |required, actual| {
                validate_class_requirement(checker, class, required, actual)
            }) {
                failure = Some((composer, error));
                break;
            }
        }
        if failure.is_none() && binding_failure.is_none() {
            failure = contract.required.iter().find_map(|required| {
                let actual = class.methods.iter().find(|method| {
                    !method.is_abstract && method.is_static == required.method.is_static
                        && php_symbol_key(&method.name) == php_symbol_key(&required.method.name)
                })?;
                validate_class_requirement(checker, class, &required.method, actual)
                    .err()
                    .map(|error| (position_of(&positions, &class.name), error))
            });
        }
        if let Some((position, error)) = failure {
            match slot_after(position) {
                Some(index) => replacements.push((index, (true, position), error)),
                None => errors.extend(error.flatten().into_iter().map(|error| ((true, position), error))),
            }
        }
    }
    for enum_unit in enums.iter().filter(|enum_unit| !enum_unit.used_traits.is_empty()) {
        let Some(uses) = enum_trait_uses(program, &enum_unit.name) else { continue };
        if let Some((position, error)) =
            enum_requirement_error(checker, &traits, &mut walk, &mut checks, &positions, enum_unit, uses)
        {
            errors.extend(error.flatten().into_iter().map(|error| ((true, position), error)));
        }
    }
    errors.extend(unchecked_trait_requirement_errors(checker, program, &traits, &mut walk, &checks, &positions));
    apply_class_error_edits(linked, replacements, removals);
    linked.extend(errors);
}

/// The first incompatible requirement of one enum, with the source position of the declaration
/// PHP reports it for: a pair a used trait satisfied itself (in trait declaration order, at that
/// trait, unless an earlier user checked it), then a requirement the enum's own method or a
/// synthesized `from`/`tryFrom` factory satisfies (at the enum).
fn enum_requirement_error(
    checker: &Checker,
    traits: &HashMap<String, TraitMembers<'_>>,
    walk: &mut RequirementWalk<'_, '_>,
    checks: &mut PairChecks,
    positions: &HashMap<String, usize>,
    enum_unit: &FlattenedClass,
    uses: &[TraitUse],
) -> Option<(usize, CompileError)> {
    let contract = walk.uses_contract(uses);
    for pair in in_declaration_order(&contract.satisfied, positions) {
        if checks.checked.contains(&pair.key()) {
            continue;
        }
        if let Err(error) = checks.check(pair, traits, |required, actual| {
            validate_requirement(checker, enum_unit, required, actual)
        }) {
            return Some((position_of(positions, &pair.composer), error));
        }
    }
    let position = position_of(positions, &enum_unit.name);
    contract.required.iter().map(|required| &required.method).find_map(|required| {
        let outcome = if let Some(actual) = enum_unit.methods.iter().find(|method| {
            !method.is_abstract && method.is_static == required.is_static
                && php_symbol_key(&method.name) == php_symbol_key(&required.name)
        }) {
            validate_requirement(checker, enum_unit, required, actual)
        } else if required.is_static {
            match super::enum_factories::public_signature(checker, &enum_unit.name, &required.name) {
                Some(actual) => super::enum_factories::validate_trait_requirement(
                    checker, &enum_unit.name, required, &actual,
                ),
                None => Ok(()),
            }
        } else {
            Ok(())
        };
        outcome.err().map(|error| (position, error))
    })
}

impl PairChecks {
    /// Checks `pair` with `validate`, which receives the required and the satisfying method
    /// (a pair of two kinds fails without it), and records the outcome: a pair checked once for
    /// all users is marked checked, and a failure marks its composing trait.
    fn check(
        &mut self,
        pair: &SatisfiedPair,
        traits: &HashMap<String, TraitMembers<'_>>,
        validate: impl FnOnce(&ClassMethod, &ClassMethod) -> Result<(), CompileError>,
    ) -> Result<(), CompileError> {
        let recorded = pair.is_user_independent();
        if recorded {
            self.checked.insert(pair.key());
        }
        let outcome = match pair.kind_error(traits) {
            Some(error) => Err(error),
            None => validate(&pair.required.method, &pair.actual.method),
        };
        if outcome.is_err() {
            if recorded {
                self.failed.insert(pair.key());
            }
            self.failed_traits.insert(pair.composer.clone());
        }
        outcome
    }
}

/// Checks the requirements each trait satisfied itself that no class or enum walk checked (the
/// trait is unused, or its users failed to build), which PHP checks when it declares the trait:
/// the first incompatible pair of each such trait, in declaration order, named after the trait.
/// A trait that already failed is not reported again. Pairs naming `self`, `parent` or `static`
/// resolve against a consuming class and are left to it.
fn unchecked_trait_requirement_errors(
    checker: &Checker,
    program: &Program,
    traits: &HashMap<String, TraitMembers<'_>>,
    walk: &mut RequirementWalk<'_, '_>,
    checks: &PairChecks,
    positions: &HashMap<String, usize>,
) -> Vec<(LinkOrder, CompileError)> {
    let mut declared: Vec<(usize, String)> = Vec::new();
    collect_trait_names(program, &mut declared, positions);
    declared.sort_by_key(|(position, _)| *position);
    let mut errors = Vec::new();
    for (position, name) in declared {
        let trait_key = php_symbol_key(&name);
        if checks.failed_traits.contains(&trait_key) {
            continue;
        }
        let owner = FlattenedClass {
            name: name.clone(),
            span: Span::dummy(),
            extends: None,
            implements: Vec::new(),
            is_abstract: true,
            is_final: false,
            is_readonly_class: false,
            properties: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constants: Vec::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        };
        let contract = walk.trait_contract(&trait_key);
        for pair in contract.satisfied.iter().filter(|pair| pair.composer == trait_key) {
            let key = pair.key();
            if !pair.is_user_independent()
                || checks.checked.contains(&key)
                || checks.suppressed.contains(&key)
            {
                continue;
            }
            let outcome = match pair.kind_error(traits) {
                Some(error) => Err(error),
                None => validate_class_requirement(checker, &owner, &pair.required.method, &pair.actual.method),
            };
            if let Err(error) = outcome {
                errors.extend(error.flatten().into_iter().map(|error| ((true, position), error)));
                break;
            }
        }
    }
    errors
}

/// Collects each trait declaration's name with its source position, through namespace blocks.
fn collect_trait_names(
    program: &[crate::parser::ast::Stmt],
    names: &mut Vec<(usize, String)>,
    positions: &HashMap<String, usize>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => names.push((position_of(positions, name), name.clone())),
            StmtKind::NamespaceBlock { body, .. } => collect_trait_names(body, names, positions),
            _ => {}
        }
    }
}

/// The source position of each class, enum and trait declaration, keyed by `php_symbol_key`,
/// counted through namespace blocks: the position half of a `LinkOrder`.
pub(crate) fn declaration_positions(program: &Program) -> HashMap<String, usize> {
    /// Numbers the declarations of `stmts` from `next`, recursing into namespace blocks.
    fn walk(stmts: &[crate::parser::ast::Stmt], next: &mut usize, positions: &mut HashMap<String, usize>) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::ClassDecl { name, .. }
                | StmtKind::EnumDecl { name, .. }
                | StmtKind::TraitDecl { name, .. } => {
                    positions.entry(php_symbol_key(name)).or_insert(*next);
                    *next += 1;
                }
                StmtKind::NamespaceBlock { body, .. } => walk(body, next, positions),
                _ => {}
            }
        }
    }
    let mut positions = HashMap::new();
    walk(program, &mut 0, &mut positions);
    positions
}

/// The source position of the declaration `name`, or the end for one not declared in source.
pub(crate) fn position_of(positions: &HashMap<String, usize>, name: &str) -> usize {
    positions.get(&php_symbol_key(name)).copied().unwrap_or(usize::MAX)
}

/// `pairs` ordered by the source position of the trait that composed each, keeping `use`
/// order among the pairs of one trait.
fn in_declaration_order<'p>(
    pairs: &'p [SatisfiedPair],
    positions: &HashMap<String, usize>,
) -> Vec<&'p SatisfiedPair> {
    let mut ordered: Vec<&SatisfiedPair> = pairs.iter().collect();
    ordered.sort_by_key(|pair| position_of(positions, &pair.composer));
    ordered
}

/// Applies the class error edits of `validate_trait_requirements` to `linked`: each replacement
/// puts a used trait's own requirement incompatibility, with its order, in place of a class's
/// error PHP reports after declaring that trait, and each removal drops a class's error whose
/// trait incompatibility an earlier class already reported. Two classes can share one error
/// (their common parent's), so a slot that holds a replacement is never removed, and it keeps
/// the replacement of the trait declared first, which PHP reports.
fn apply_class_error_edits(
    linked: &mut Vec<(LinkOrder, CompileError)>,
    replacements: Vec<(usize, LinkOrder, CompileError)>,
    mut removals: Vec<usize>,
) {
    removals.retain(|index| !replacements.iter().any(|(replaced, _, _)| replaced == index));
    let mut chosen: HashMap<usize, (LinkOrder, CompileError)> = HashMap::new();
    for (index, order, error) in replacements {
        match chosen.get(&index) {
            Some((kept, _)) if *kept <= order => {}
            _ => {
                chosen.insert(index, (order, error));
            }
        }
    }
    for (index, entry) in chosen {
        if let Some(slot) = linked.get_mut(index) {
            *slot = entry;
        }
    }
    removals.sort_unstable();
    removals.dedup();
    for index in removals.into_iter().rev() {
        if index < linked.len() {
            linked.remove(index);
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

/// A requirement a trait satisfied itself: the abstract method, the method that satisfies it,
/// and the key of the trait whose composition paired them, where PHP checks the pair. With
/// `kind_mismatch`, the method has the name but not the kind (static or not) of the requirement:
/// it cannot satisfy it, and PHP rejects the composing trait.
#[derive(Clone)]
struct SatisfiedPair {
    required: DeclaredMethod,
    actual: DeclaredMethod,
    composer: String,
    kind_mismatch: bool,
}

impl SatisfiedPair {
    /// The identity of the pair, shared by its copies along several `use` paths.
    fn key(&self) -> RequirementPairKey {
        (self.required.identity(), self.actual.identity())
    }

    /// Whether the pair's outcome is the same for every class or enum using the trait, so it is
    /// checked once: a kind mismatch, or a pair free of `self`, `parent` and `static`, which
    /// resolve against each consuming class.
    fn is_user_independent(&self) -> bool {
        self.kind_mismatch
            || (!mentions_relative_class_types(&self.required.method)
                && !mentions_relative_class_types(&self.actual.method))
    }

    /// PHP's error for a kind mismatch, named after the declared traits as in php 8.5.12's
    /// `Cannot make non static method N::bar() static in class T`; `None` for a pair of one kind.
    fn kind_error(&self, traits: &HashMap<String, TraitMembers<'_>>) -> Option<CompileError> {
        if !self.kind_mismatch {
            return None;
        }
        let name = |key: &str| traits.get(key).map_or_else(|| key.to_string(), |(name, _, _)| name.to_string());
        let (from, to) = if self.required.method.is_static {
            ("static", "non static")
        } else {
            ("non static", "static")
        };
        Some(CompileError::new(
            self.actual.method.span,
            &format!(
                "Cannot make {} method {}::{}() {} in class {}",
                from,
                name(&self.required.origin),
                self.required.method.name,
                to,
                name(&self.composer),
            ),
        ))
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
    satisfied: Vec<SatisfiedPair>,
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
        self.satisfied.retain(|pair| seen.insert(pair.key()));
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
    /// A method of the name but the other kind (static or not) is paired with the requirement
    /// as a kind mismatch, and the requirement still reaches the users, as flattening keeps it.
    /// Each trait is computed once.
    fn trait_contract(&mut self, trait_key: &str) -> TraitContract {
        if let Some(contract) = self.done.get(trait_key) {
            return contract.clone();
        }
        let traits = self.traits;
        let Some((_, nested, methods)) = traits.get(trait_key) else { return TraitContract::default() };
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
            let named = |method: &&DeclaredMethod| {
                php_symbol_key(&method.method.name) == php_symbol_key(&required.method.name)
            };
            match contract.provided.iter().find(|method| method.satisfies(&required.method)) {
                Some(actual) => contract.satisfied.push(SatisfiedPair {
                    required,
                    actual: actual.clone(),
                    composer: trait_key.to_string(),
                    kind_mismatch: false,
                }),
                None => {
                    if let Some(actual) = contract.provided.iter().find(named) {
                        contract.satisfied.push(SatisfiedPair {
                            required: required.clone(),
                            actual: actual.clone(),
                            composer: trait_key.to_string(),
                            kind_mismatch: true,
                        });
                    }
                    contract.required.push(required);
                }
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
                traits.insert(php_symbol_key(name), (name.as_str(), trait_uses, methods));
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
