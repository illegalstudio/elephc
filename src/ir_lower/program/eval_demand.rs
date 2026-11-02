//! Purpose:
//! Computes which names the module's bridge-run `eval` fragments can reach, so the eval-only
//! builtin surfaces (the Reflection classes, the DateTime alias set) are lowered in proportion
//! to the fragments instead of whole whenever the bridge links.
//!
//! Called from:
//! - `crate::ir_lower::reflection::referenced_builtin_reflection_classes`.
//! - `crate::ir_lower::builtin_datetime::eval_fragments_may_reach_dates`.
//!
//! Key details:
//! - A literal fragment that parses contributes the names the reachability scanner finds in it,
//!   exactly as if the code were inline. Any dynamic hazard inside it, an eval whose source is
//!   only known at runtime, a dynamic callable op, or a fragment that fails to parse makes the
//!   demand `Everything`: the former unconditional answer.
//! - Fragments the EIR AOT planner compiles natively contribute nothing: their references are
//!   ordinary lowered EIR, which the existing per-surface scans already see.
//! - Names are `php_symbol_key`-folded, like every reachability index.

use std::collections::HashSet;

use super::*;
use crate::optimize::reachability::usage;

/// What the bridge-run eval fragments of one module can reach by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::ir_lower) enum EvalDemand {
    /// No fragment needs the bridge.
    #[default]
    None,
    /// Only these case-folded names can be reached.
    Names(EvalNames),
    /// Some fragment resolves names at runtime, so every declaration may be reached.
    Everything,
}

/// The case-folded names literal fragments can reach.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::ir_lower) struct EvalNames {
    pub(in crate::ir_lower) functions: HashSet<String>,
    pub(in crate::ir_lower) classes: HashSet<String>,
    pub(in crate::ir_lower) methods: HashSet<String>,
    /// Methods called on a receiver whose class the fragment scan could not name, such as an
    /// object the outer program built: `$d->setDate()` with `$d = new DateTime(...)` inline.
    pub(in crate::ir_lower) wildcard_methods: HashSet<String>,
}

impl EvalDemand {
    /// Returns true when the fragments can reach any declaration at all.
    pub(in crate::ir_lower) fn is_everything(&self) -> bool {
        matches!(self, EvalDemand::Everything)
    }

    /// Returns true when the fragments may reach the named class (any spelling of its case).
    pub(in crate::ir_lower) fn names_class(&self, class_name: &str) -> bool {
        match self {
            EvalDemand::None => false,
            EvalDemand::Everything => true,
            EvalDemand::Names(names) => names.classes.contains(&php_symbol_key(class_name)),
        }
    }

    /// Returns true when a method the fragments call on a receiver of unknown class satisfies
    /// the predicate. Such a receiver can be an object the outer program built, so the method
    /// may belong to a class the fragment never spells.
    pub(in crate::ir_lower) fn names_wildcard_method(
        &self,
        method_matches: impl Fn(&str) -> bool,
    ) -> bool {
        match self {
            EvalDemand::None => false,
            EvalDemand::Everything => true,
            EvalDemand::Names(names) => {
                names.wildcard_methods.iter().any(|name| method_matches(name))
            }
        }
    }

    /// Returns true when some reachable class or function name satisfies the predicates.
    pub(in crate::ir_lower) fn names_any(
        &self,
        class_matches: impl Fn(&str) -> bool,
        function_matches: impl Fn(&str) -> bool,
    ) -> bool {
        match self {
            EvalDemand::None => false,
            EvalDemand::Everything => true,
            EvalDemand::Names(names) => {
                names.classes.iter().any(|name| class_matches(name))
                    || names.functions.iter().any(|name| function_matches(name))
            }
        }
    }
}

/// Computes the eval demand of a lowered module from its eval instructions.
pub(in crate::ir_lower) fn eval_demand(module: &Module) -> EvalDemand {
    let mut names = EvalNames::default();
    let mut any_bridge_fragment = false;
    for function in all_lowered_functions(module) {
        for (inst_index, inst) in function.instructions.iter().enumerate() {
            if !instruction_uses_eval(module, inst) {
                continue;
            }
            // The non-literal eval ops name their target statically: `f()` where `f` is only
            // declared by a fragment, `new C`, `C::m()`, `function_exists('f')`, a constant.
            // Lowering emits them only for names no declaration owns, so they can reach nothing
            // but what a fragment declares; recording the name keeps them off the widening path.
            if let Some(named) = named_dynamic_op_target(module, inst) {
                any_bridge_fragment = true;
                match named {
                    NamedTarget::Function(name) => {
                        names.functions.insert(name);
                    }
                    NamedTarget::Class(name) => {
                        names.classes.insert(name);
                    }
                    NamedTarget::StaticMethod(class, method) => {
                        names.classes.insert(class);
                        names.methods.insert(method);
                    }
                    NamedTarget::Constant => {}
                }
                continue;
            }
            // Everything else is the `eval()` construct itself. Only a literal source can be
            // read; a runtime-sourced one can reach anything.
            if inst.op != Op::EvalLiteralCall {
                return EvalDemand::Everything;
            }
            if !eval_literal_call_requires_bridge(module, function, inst_index, inst) {
                continue;
            }
            any_bridge_fragment = true;
            let Some(fragment) = eval_literal_fragment_from_inst(module, inst) else {
                return EvalDemand::Everything;
            };
            let Some(program) = crate::eval_aot::parse_literal_fragment(
                &fragment,
                eval_literal_strict_profile(inst),
            ) else {
                return EvalDemand::Everything;
            };
            let found = usage::scan_eval_fragment(&program);
            // A dynamic METHOD hazard counts too: `$d?->$m()` on an object the outer program
            // built can pick any method of a class the fragment never spells (a `DateTime`
            // alias body, a Reflection accessor), so only the whole surface is safe.
            if found.hazards.dynamic_function
                || found.hazards.dynamic_method
                || found.hazards.dynamic_class
                || found.hazards.enumerates_functions
            {
                return EvalDemand::Everything;
            }
            names.functions.extend(found.functions);
            names.classes.extend(found.instantiated_classes);
            names
                .classes
                .extend(found.methods.iter().map(|(class, _, _)| class.clone()));
            names
                .methods
                .extend(found.methods.into_iter().map(|(_, method, _)| method));
            for (method, _) in found.wildcard_methods {
                names.methods.insert(method.clone());
                names.wildcard_methods.insert(method);
            }
            names.classes.extend(found.classes);
        }
    }
    if !any_bridge_fragment {
        return EvalDemand::None;
    }
    // A builtin the fragment calls can hand back objects the fragment never names:
    // `class_get_attributes()` returns `ReflectionAttribute` instances, so that class has to be
    // lowered for the fragment to use the result. The registry signatures carry those types.
    let returned: Vec<String> = names
        .functions
        .iter()
        .flat_map(|name| builtin_returned_classes(name))
        .collect();
    names.classes.extend(returned);
    EvalDemand::Names(names)
}

/// Returns the case-folded classes a builtin function can return, from its registry signature
/// plus the builtins whose contract says `mixed` while their checker refines the type.
fn builtin_returned_classes(name: &str) -> Vec<String> {
    let mut classes = crate::builtins::registry::function_sig(name)
        .map(|signature| class_names_in_type(&signature.return_type))
        .unwrap_or_default();
    // `class_get_attributes` is contracted as `mixed`; its `check` returns
    // `Array(Object("ReflectionAttribute"))` (see `builtins::system::class_get_attributes`).
    if name == "class_get_attributes" {
        classes.push(php_symbol_key("ReflectionAttribute"));
    }
    classes
}

/// Collects the case-folded class names a type can carry, through unions and arrays.
fn class_names_in_type(ty: &PhpType) -> Vec<String> {
    match ty {
        PhpType::Object(name) | PhpType::Packed(name) => vec![php_symbol_key(name)],
        PhpType::Pointer(Some(name)) => vec![php_symbol_key(name)],
        PhpType::Array(inner) | PhpType::Buffer(inner) => class_names_in_type(inner),
        PhpType::AssocArray { key, value } => {
            let mut names = class_names_in_type(key);
            names.extend(class_names_in_type(value));
            names
        }
        PhpType::Union(members) => members.iter().flat_map(class_names_in_type).collect(),
        _ => Vec::new(),
    }
}

/// The statically named target of a non-literal eval op.
enum NamedTarget {
    Function(String),
    Class(String),
    StaticMethod(String, String),
    Constant,
}

/// Returns the name a non-literal eval op targets, or `None` for `EvalLiteralCall` and the
/// `eval()` construct itself, whose source decides what they reach.
fn named_dynamic_op_target(module: &Module, inst: &crate::ir::Instruction) -> Option<NamedTarget> {
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => {
            Some(data.as_raw() as usize)
        }
        _ => None,
    };
    match inst.op {
        Op::EvalFunctionCall | Op::EvalFunctionCallArray | Op::EvalFunctionExists => {
            let name = module.data.function_names.get(data?)?;
            Some(NamedTarget::Function(php_symbol_key(name.trim_start_matches('\\'))))
        }
        Op::EvalObjectNew | Op::EvalClassExists => {
            let name = module.data.class_names.get(data?)?;
            Some(NamedTarget::Class(php_symbol_key(name.trim_start_matches('\\'))))
        }
        Op::EvalStaticMethodCall => {
            let target = module.data.strings.get(data?)?;
            let (class, method) = target.rsplit_once("::")?;
            Some(NamedTarget::StaticMethod(
                php_symbol_key(class.trim_start_matches('\\')),
                php_symbol_key(method),
            ))
        }
        Op::EvalConstantExists | Op::EvalConstantFetch => Some(NamedTarget::Constant),
        _ => None,
    }
}

/// Returns true when one instruction requires eval runtime support.
pub(in crate::ir_lower) fn instruction_uses_eval(
    module: &Module,
    inst: &crate::ir::Instruction,
) -> bool {
    matches!(
        inst.op,
        Op::EvalLiteralCall
            | Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::EvalObjectNew
            | Op::EvalStaticMethodCall
            | Op::EvalFunctionExists
            | Op::EvalClassExists
            | Op::EvalConstantExists
            | Op::EvalConstantFetch
    ) || builtin_call_is_eval(module, inst)
}

/// Returns true when one lowered builtin call is PHP's `eval` construct.
fn builtin_call_is_eval(module: &Module, inst: &crate::ir::Instruction) -> bool {
    if inst.op != Op::LanguageConstructCall {
        return false;
    }
    // A profiled call carries the SAME construct name, in a different immediate shape. Matching
    // only the bare form let `eval($computed)` past this filter unseen.
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return false,
    };
    module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .is_some_and(|name| php_symbol_key(name.trim_start_matches('\\')) == "eval")
}
