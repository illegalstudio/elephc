//! Purpose:
//! Synthesises the built-in reflection class checker metadata so user code can
//! receive `ReflectionAttribute` instances and query class/member attributes
//! through a small PHP-compatible Reflection surface.
//!
//! Called from:
//! - `crate::types::checker::driver::init` (alongside `inject_builtin_throwables`).
//!
//! Key details:
//! - Property and method bodies are dummies, private-slot accessors, or small
//!   fallbacks; runtime population is handled by codegen-only reflection constructors.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::names::Name;
use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, InstanceOfTarget, Stmt,
    StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;
use crate::types::{ClassInfo, PhpType};

use super::super::Checker;

/// Returns a dummy source span for synthetic reflection AST nodes.
fn dummy() -> crate::span::Span {
    crate::span::Span::dummy()
}

use super::InterfaceDeclInfo;

mod gate;
pub(crate) use gate::REFLECTION_CLASS_NAMES;
mod injection;
mod ast_literals;
mod basic_methods;
mod function_parameter_classes;
mod reference_class;
mod type_classes;
mod class_model;
mod class_table_ctor;
mod member_table_ctor;
mod class_scalar_methods;
mod class_relations;
mod class_collections;
mod member_predicates;
mod property_values;
mod owner_class;
mod owner_methods;
mod owner_helpers;
mod signature_patches;
mod signature_patch_class;
mod signature_patch_common;
mod signature_patch_members;
mod signature_patch_types;

#[allow(unused_imports)]
use injection::*;
#[allow(unused_imports)]
use ast_literals::*;
#[allow(unused_imports)]
use basic_methods::*;
#[allow(unused_imports)]
use function_parameter_classes::*;
#[allow(unused_imports)]
use reference_class::*;
#[allow(unused_imports)]
use type_classes::*;
#[allow(unused_imports)]
use class_model::*;
#[allow(unused_imports)]
use class_scalar_methods::*;
#[allow(unused_imports)]
use class_relations::*;
#[allow(unused_imports)]
use class_collections::*;
#[allow(unused_imports)]
use member_predicates::*;
#[allow(unused_imports)]
use property_values::*;
#[allow(unused_imports)]
use owner_class::*;
#[allow(unused_imports)]
use owner_methods::*;
#[allow(unused_imports)]
use owner_helpers::*;
#[allow(unused_imports)]
use signature_patches::*;
#[allow(unused_imports)]
use signature_patch_class::*;
#[allow(unused_imports)]
use signature_patch_common::*;
#[allow(unused_imports)]
use signature_patch_members::*;
#[allow(unused_imports)]
use signature_patch_types::*;

pub(crate) use gate::program_may_reference_reflection;
pub(crate) use injection::inject_builtin_reflection;
pub(crate) use signature_patches::patch_builtin_reflection_signatures;

/// Returns the private metadata slot backing a public built-in Reflection property.
///
/// These properties are exposed by PHP itself but share storage with the synthetic accessors,
/// so the compiler must not allocate or initialize a second object slot for them.
pub(crate) fn reflection_virtual_property_backing(
    class_name: &str,
    property: &str,
) -> Option<&'static str> {
    let class_name = class_name.trim_start_matches('\\');
    match property {
        "name"
            if matches!(
                class_name,
                // The INTERFACE too: every implementation of `Reflector` carries a public
                // `$name` — ReflectionClass/Object/Enum, the function abstracts, Property,
                // Parameter, ClassConstant, the enum cases, and the two extension reflectors.
                // Symfony's `ReflectionCaster` reads `$m->name` off a value narrowed to
                // `\Reflector`, which is valid PHP and was refused.
                "Reflector"
                    | "ReflectionClass"
                    | "ReflectionObject"
                    | "ReflectionEnum"
                    | "ReflectionFunctionAbstract"
                    | "ReflectionFunction"
                    | "ReflectionMethod"
                    | "ReflectionProperty"
                    | "ReflectionParameter"
                    | "ReflectionClassConstant"
                    | "ReflectionEnumUnitCase"
                    | "ReflectionEnumBackedCase"
            ) =>
        {
            Some("__name")
        }
        "class"
            if matches!(
                class_name,
                "ReflectionFunctionAbstract"
                    | "ReflectionMethod"
                    | "ReflectionProperty"
                    | "ReflectionClassConstant"
                    | "ReflectionEnumUnitCase"
                    | "ReflectionEnumBackedCase"
            ) =>
        {
            Some("__class")
        }
        _ => None,
    }
}

/// Walks a class's parent chain to the built-in Reflection class that owns its virtual
/// `$name` / `$class` properties, or `None` when no ancestor is one.
///
/// A user subclass reads the inherited properties exactly as the built-in does -- Doctrine's
/// `ReflectionReadonlyProperty extends ReflectionProperty` formats `$this->class` and
/// `$this->name` -- yet only the built-in names were recognised, so the read was an
/// "Undefined property". `parent_of` answers one hop for whichever class table the caller owns.
pub(crate) fn reflection_virtual_owner(
    class_name: &str,
    parent_of: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let mut current = class_name.trim_start_matches('\\').to_string();
    for _ in 0..64 {
        if reflection_virtual_property_backing(&current, "name").is_some() {
            return Some(current);
        }
        current = parent_of(&current)?.trim_start_matches('\\').to_string();
    }
    None
}

/// [`reflection_virtual_property_backing`] for a class that may merely EXTEND a Reflection class.
pub(crate) fn reflection_virtual_property_backing_in(
    class_name: &str,
    property: &str,
    parent_of: impl Fn(&str) -> Option<String>,
) -> Option<&'static str> {
    if !matches!(property, "name" | "class") {
        return None;
    }
    let owner = reflection_virtual_owner(class_name, parent_of)?;
    reflection_virtual_property_backing(&owner, property)
}

/// [`reflection_virtual_property_type`] for a class that may merely EXTEND a Reflection class.
pub(crate) fn reflection_virtual_property_type_in(
    class_name: &str,
    property: &str,
    parent_of: impl Fn(&str) -> Option<String>,
) -> Option<PhpType> {
    if !matches!(property, "name" | "class") {
        return None;
    }
    let owner = reflection_virtual_owner(class_name, parent_of)?;
    reflection_virtual_property_type(&owner, property)
}

/// Returns the PHP-visible type of a virtual built-in Reflection property.
pub(crate) fn reflection_virtual_property_type(
    class_name: &str,
    property: &str,
) -> Option<PhpType> {
    let class_name = class_name.trim_start_matches('\\');
    match property {
        "name" if reflection_virtual_property_backing(class_name, property).is_some() => {
            Some(PhpType::Str)
        }
        "class"
            if matches!(
                class_name,
                "ReflectionMethod"
                    | "ReflectionProperty"
                    | "ReflectionClassConstant"
                    | "ReflectionEnumUnitCase"
                    | "ReflectionEnumBackedCase"
            ) =>
        {
            Some(PhpType::Str)
        }
        "class" if class_name == "ReflectionFunctionAbstract" => {
            Some(PhpType::Union(vec![PhpType::Str, PhpType::Void]))
        }
        _ => None,
    }
}
