//! Purpose:
//! Plans and lowers the synthetic EIR applicators that apply PHP 8.5
//! `clone($object, $withProperties)` overrides, one body per
//! `(runtime class, invocation-scope profile)` pair.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` after class methods and property initializers exist.
//!
//! Key details:
//! - Applicator bodies are ORDINARY PHP statements lowered by the ordinary pipeline, so every
//!   override write inherits typed weak coercion, set hooks, `__set`, dynamic-property storage
//!   and ownership from the same code paths a hand-written `$obj->p = $v` uses.
//! - Scope resolution is precomputed here, not at run time: `arms::resolve()` answers for one
//!   `(class, scope, name)` triple and identical answers across scopes collapse into a single
//!   body. The group containing global scope is the default arm for every unlisted scope.
//! - Only a scope in the clone class's own FAMILY, the class itself, its ancestors and its
//!   descendants, can resolve any name differently from global scope: private shadowing needs
//!   the clone to be an instance of the scope, and both protected access and protected `set`
//!   visibility go through php's ancestor-or-descendant test. Every other scope therefore shares
//!   the default body instead of multiplying the generated code by the class count.
//! - The planner only runs when the module can actually reach a two-argument `clone()`: either a
//!   lowered `RuntimeFnId::CloneWith` carries an override operand, or the data pool interned the
//!   name `clone`, which is how a first-class callable or `call_user_func('clone', …)` reaches
//!   the backend's builtin callable wrapper.
//! - A property whose backend storage cannot accept a runtime-shaped value (pointers, buffers,
//!   resources, callables, packed fields) gets an explicit refusal arm. Overrides are never
//!   silently dropped, and the applicator never asks the backend for a store it cannot emit.
//! - Only classes this program DECLARES are planned. A checker-injected builtin is skipped
//!   entirely: the checker never widens its slots, and its clones fall to the existing catchable
//!   "property overrides are not supported for this class" runtime Error instead.

mod arms;
mod body;
mod scoped_setters;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ir::{CloneOverrideApplicator, Immediate, Module, Op, RuntimeCallTarget, RuntimeFnId};
use crate::parser::ast::ExprKind;
use crate::types::{CheckResult, ClassInfo, FunctionSig, PhpType};

use arms::OverrideArm;
use body::ResolvedArm;

/// Where a lowered module can reach `clone()` with an override array.
#[derive(Default)]
struct CloneOverrideSites {
    /// A site passes an object whose class is only known at run time.
    any_runtime_class: bool,
    /// A site's invocation scope is only known at run time (callable dispatch).
    runtime_scope: bool,
    /// Statically known object classes passed to two-argument `clone()`.
    concrete_classes: BTreeSet<String>,
    /// Statically known invocation scopes, with `None` for global scope.
    static_scopes: BTreeSet<Option<String>>,
}

/// Returns the reserved EIR function name of one applicator body.
///
/// The name goes through `crate::names::internal_generated_function_name`, so a PHP program that
/// declares `function _clone_apply_4_0()` cannot shadow the generated applicator. It used to:
/// `lower_clone_override_function` skips a name the module already defines, so the user body was
/// called with `(clone, overrides)` and every override was dropped.
fn applicator_function_name(class_id: u64, index: usize) -> String {
    crate::names::internal_generated_function_name("clone_apply", &[class_id, index as u64])
}

/// Generates every `clone()` override applicator the lowered module can reach.
///
/// Planning is a separate pass from lowering because an applicator arm can CALL a scoped setter
/// helper. The helper's signature has to exist in the function table the applicator body is
/// lowered against, otherwise the call would not resolve to the generated symbol at all.
pub(crate) fn lower_clone_override_applicators(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
) -> Result<(), crate::ir_lower::LoweringError> {
    let Some(sites) = collect_sites(module) else {
        return Ok(());
    };
    let mut planned = Vec::new();
    for class_name in candidate_classes(module, &sites) {
        let Some(class_info) = module.class_infos.get(&class_name).cloned() else {
            continue;
        };
        let groups = plan_class(module, &sites, &class_name, &class_info);
        planned.push((class_name, class_info.class_id, groups));
    }
    let mut functions = check_result.functions.clone();
    for (_, _, groups) in &planned {
        for group in groups {
            for (_, arm) in &group.arms {
                let OverrideArm::AssignScoped { scope, property } = arm else {
                    continue;
                };
                let Some((name, sig)) = scoped_setters::lower(
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                    scope,
                    property,
                ) else {
                    continue;
                };
                functions.insert(name, sig);
            }
        }
    }
    for (class_name, class_id, groups) in planned {
        for (index, group) in groups.into_iter().enumerate() {
            let function_name = applicator_function_name(class_id, index);
            let mut arms = Vec::with_capacity(group.arms.len());
            for (property, arm) in group.arms {
                let scoped_helper = match &arm {
                    OverrideArm::AssignScoped {
                        scope,
                        property: scoped_property,
                    } => Some(resolve_scoped_helper(
                        module,
                        &functions,
                        scope,
                        scoped_property,
                    )?),
                    _ => None,
                };
                arms.push(ResolvedArm {
                    property,
                    arm,
                    scoped_helper,
                });
            }
            let statements = body::build(&class_name, &arms, &group.unknown);
            crate::ir_lower::function::lower_clone_override_function(
                &function_name,
                &[
                    (
                        body::THIS_PARAM.to_string(),
                        PhpType::Object(class_name.clone()),
                    ),
                    (body::OVERRIDES_PARAM.to_string(), PhpType::Mixed),
                ],
                group.scope.as_deref(),
                &statements,
                module,
                check_result,
                &functions,
                constants,
                fiber_return_sigs,
            );
            module
                .clone_override_applicators
                .push(CloneOverrideApplicator {
                    class_id,
                    class_name: class_name.clone(),
                    scope_class_ids: group.scope_class_ids,
                    is_default_scope: group.is_default_scope,
                    function_name,
                });
        }
    }
    module
        .clone_override_applicators
        .sort_by(|left, right| left.function_name.cmp(&right.function_name));
    Ok(())
}

/// Returns the scoped setter helper one `AssignScoped` arm must call, or refuses the build.
///
/// The arm exists precisely because php resolves the name through an ANCESTOR's private slot,
/// which the clone's own receiver cannot address: `$this->p = $v` inside the applicator writes
/// the child's shadowing slot instead. Falling back to that write is therefore never a
/// degradation, it is a different property, so a helper the planner failed to generate is a
/// compiler defect and stops the build rather than silently writing the wrong slot.
fn resolve_scoped_helper(
    module: &Module,
    functions: &HashMap<String, FunctionSig>,
    scope: &str,
    property: &str,
) -> Result<String, crate::ir_lower::LoweringError> {
    let helper = module
        .class_infos
        .get(scope)
        .and_then(|info| {
            info.visible_property_index(property)
                .map(|slot| scoped_setters::helper_name(info.class_id, slot))
        })
        .filter(|name| functions.contains_key(name));
    helper.ok_or_else(|| {
        crate::ir_lower::LoweringError::Unsupported(crate::errors::CompileError::new(
            module
                .class_infos
                .get(scope)
                .map(|info| info.declaration_span)
                .unwrap_or_else(crate::span::Span::dummy),
            &format!(
                "clone() override planning could not generate the scoped setter for the private \
                 property {}::${} selected by that invocation scope",
                scope, property
            ),
        ))
    })
}

/// One applicator body plus the invocation scopes it is exact for.
struct ApplicatorGroup {
    /// Representative scope whose lexical class the body is lowered with.
    scope: Option<String>,
    /// Declared property arms, in deterministic name order.
    arms: Vec<(String, OverrideArm)>,
    /// What a key matching no declared name does.
    unknown: OverrideArm,
    /// Scopes served in addition to the representative one.
    scope_class_ids: Vec<u64>,
    /// Whether this body also serves global scope and every unlisted scope.
    is_default_scope: bool,
}

/// Resolves every candidate scope for one class and collapses identical profiles.
fn plan_class(
    module: &Module,
    sites: &CloneOverrideSites,
    class_name: &str,
    class_info: &ClassInfo,
) -> Vec<ApplicatorGroup> {
    let names = override_arm_names(module, class_info);
    let mut groups: BTreeMap<(Vec<(String, OverrideArm)>, OverrideArm), ApplicatorGroup> =
        BTreeMap::new();
    for scope in candidate_scopes(module, sites, class_name) {
        let scope_ref = scope.as_deref();
        let arms = names
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    resolve_arm(module, class_name, class_info, scope_ref, name),
                )
            })
            .collect::<Vec<_>>();
        let unknown = arms::undefined_arm(&module.class_infos, class_name, class_info, scope_ref);
        let scope_class_id = scope_ref.and_then(|name| {
            module
                .class_infos
                .get(name)
                .map(|info| info.class_id)
        });
        let entry = groups
            .entry((arms.clone(), unknown.clone()))
            .or_insert_with(|| ApplicatorGroup {
                scope: scope.clone(),
                arms,
                unknown,
                scope_class_ids: Vec::new(),
                is_default_scope: false,
            });
        match scope_class_id {
            Some(class_id) => entry.scope_class_ids.push(class_id),
            None => entry.is_default_scope = true,
        }
    }
    let mut groups = groups.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.scope_class_ids.sort_unstable();
        group.scope_class_ids.dedup();
    }
    groups
}

/// Collects the property names one class's applicator needs a dedicated arm for.
fn override_arm_names(module: &Module, class_info: &ClassInfo) -> Vec<String> {
    let mut names = class_info
        .properties
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    // A strict ancestor's private slot is invisible by name on the child, but the invocation
    // scope can still select it, so the applicator needs an arm for that name too.
    let mut ancestor = class_info.parent.clone();
    let mut guard = 0usize;
    while let Some(name) = ancestor {
        let Some(info) = module.class_infos.get(&name) else {
            break;
        };
        for (property, _) in &info.properties {
            names.insert(property.clone());
        }
        guard += 1;
        if guard > module.class_infos.len() + 1 {
            break;
        }
        ancestor = info.parent.clone();
    }
    names.into_iter().collect()
}

/// Resolves one arm and downgrades slots the backend store path cannot accept.
fn resolve_arm(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    scope: Option<&str>,
    property: &str,
) -> OverrideArm {
    let arm = arms::resolve(&module.class_infos, class_name, class_info, scope, property);
    match &arm {
        OverrideArm::AssignThis => {
            downgrade_unsupported_slot(class_name, class_info, property, arm)
        }
        OverrideArm::AssignScoped { scope, property } => {
            let Some(scope_info) = module.class_infos.get(scope) else {
                return arm.clone();
            };
            let (scope, property) = (scope.clone(), property.clone());
            downgrade_unsupported_slot(&scope, scope_info, &property, arm)
        }
        _ => arm,
    }
}

/// Refuses a write whose destination slot cannot hold a runtime-shaped override value.
fn downgrade_unsupported_slot(
    owner_name: &str,
    owner: &ClassInfo,
    property: &str,
    accepted: OverrideArm,
) -> OverrideArm {
    if owner.property_is_virtual(property)
        && !owner
            .property_hooks
            .get(property)
            .is_some_and(|hooks| hooks.set)
    {
        return OverrideArm::Deny(format!(
            "Cannot modify virtual property {}::${}",
            owner_name, property
        ));
    }
    // A reference DESTINATION is not a refusal in php: `clone($o, ["p" => 7])` where `$o->p` is a
    // reference writes THROUGH the shared cell, so every alias of it observes 7, which is why the
    // slot's own by-reference flag is deliberately not consulted here. Only a reference SOURCE
    // ELEMENT is refused, and that is decided per element rather than per slot.
    let Some((_, (_, php_type))) = owner.visible_property(property) else {
        return accepted;
    };
    if slot_accepts_runtime_value(php_type) {
        return accepted;
    }
    OverrideArm::Deny(format!(
        "Cannot modify internal property {}::${}",
        owner_name, property
    ))
}

/// Returns whether the declared storage type can be written from a boxed runtime value.
fn slot_accepts_runtime_value(php_type: &PhpType) -> bool {
    matches!(
        php_type.codegen_repr(),
        PhpType::Int
            | PhpType::Float
            | PhpType::Str
            | PhpType::Bool
            | PhpType::False
            | PhpType::Void
            | PhpType::Never
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Iterable
            | PhpType::Mixed
            | PhpType::TaggedScalar
            | PhpType::Union(_)
            | PhpType::Object(_)
    )
}

/// Returns the invocation scopes one class's applicators must distinguish.
///
/// Global scope is always present because it is the default arm. Beyond it only the clone
/// class's own family can change an answer, so an unrelated scope never needs its own body.
fn candidate_scopes(
    module: &Module,
    sites: &CloneOverrideSites,
    class_name: &str,
) -> Vec<Option<String>> {
    let mut scopes = vec![None];
    let mut family = BTreeSet::new();
    for (candidate, _) in &module.class_infos {
        if candidate == class_name
            || arms::is_subclass_of(&module.class_infos, class_name, candidate)
            || arms::is_subclass_of(&module.class_infos, candidate, class_name)
        {
            family.insert(candidate.clone());
        }
    }
    if !sites.runtime_scope {
        family.retain(|candidate| sites.static_scopes.contains(&Some(candidate.clone())));
    }
    scopes.extend(family.into_iter().map(Some));
    scopes
}

/// Returns the classes whose clones can reach an override array in this module.
fn candidate_classes(module: &Module, sites: &CloneOverrideSites) -> Vec<String> {
    let mut names = BTreeSet::new();
    if sites.any_runtime_class {
        names.extend(module.class_infos.keys().cloned());
    } else {
        for class_name in &sites.concrete_classes {
            names.insert(class_name.clone());
            for (candidate, _) in &module.class_infos {
                if arms::is_subclass_of(&module.class_infos, candidate, class_name) {
                    names.insert(candidate.clone());
                }
            }
        }
    }
    names
        .into_iter()
        // A checker-injected builtin keeps its own authoritative layout. The checker's override
        // widening in `types::checker::clone_override_storage` deliberately restamps USER classes
        // only, so planning an applicator for a builtin means writing a runtime-shaped value into
        // a slot nothing ever widened, and a slot whose declared type cannot take one (a
        // `Callable`, say) refuses the whole BUILD rather than the single write. The catalog is
        // the same authority `reserve_eval_subclass_property_storage` consults, and it already
        // contains every builtin reflection class this filter used to name one at a time.
        // A user subclass of a builtin is absent from the catalog, so it stays eligible.
        // `stdClass` is the intentional builtin exception: its authoritative representation is
        // the dynamic-property hash itself, and clone overrides must synthesize an applicator so
        // unknown keys reach that hash instead of the generic unsupported fallback.
        .filter(|name| {
            elephc_builtin_contract::lookup_class(name).is_none()
                || crate::types::checker::builtin_stdclass::is_stdclass(name)
        })
        .collect()
}

/// Scans the lowered module for reachable two-argument `clone()` sites.
fn collect_sites(module: &Module) -> Option<CloneOverrideSites> {
    let mut sites = CloneOverrideSites::default();
    let mut found = false;
    for function in crate::ir_lower::program::all_lowered_functions(module) {
        for inst in &function.instructions {
            if inst.op != Op::RuntimeCall || inst.operands.len() < 2 {
                continue;
            }
            let target = match inst.immediate {
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))) => target,
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                    target,
                    ..
                })) => target,
                _ => continue,
            };
            if target != RuntimeFnId::CloneWith {
                continue;
            }
            found = true;
            match function
                .value(inst.operands[0])
                .map(|value| value.php_type.codegen_repr())
            {
                Some(PhpType::Object(class_name)) => {
                    sites.concrete_classes.insert(class_name);
                }
                _ => sites.any_runtime_class = true,
            }
            if inst.operands.len() >= 3 {
                sites.runtime_scope = true;
            } else {
                sites.static_scopes.insert(function.lexical_class.clone());
            }
        }
    }
    // A callable reference reaches the backend's builtin wrapper, which carries both the override
    // operand and the hidden invocation scope, and no lowered instruction names it here.
    if module
        .data
        .strings
        .iter()
        .any(|value| value.eq_ignore_ascii_case("clone"))
    {
        found = true;
        sites.any_runtime_class = true;
        sites.runtime_scope = true;
    }
    found.then_some(sites)
}
