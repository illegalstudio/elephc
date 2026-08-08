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

use std::collections::{HashMap, HashSet};

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
mod injection;
mod ast_literals;
mod basic_methods;
mod function_parameter_classes;
mod type_classes;
mod class_model;
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
