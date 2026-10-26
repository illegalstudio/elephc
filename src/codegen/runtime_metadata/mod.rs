//! Purpose:
//! Collects callable and class-like runtime metadata required by emitted EIR assembly.
//!
//! Called from:
//! - `super::finalize_user_asm()` before runtime data emission.
//!
//! Key details:
//! - Separates callable metadata, class reachability, static-member references, and dynamic references.

mod callables;
mod classes;
mod dynamic_references;
mod static_members;

use super::function_variants;
use crate::codegen::emit::Emitter;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{Function, Immediate, Module, Op, ValueDef};
use crate::names::{method_symbol, php_symbol_key, static_method_symbol};
use crate::types::{ClassInfo, FunctionSig, InterfaceInfo, PhpType};
use std::collections::{HashMap, HashSet};

pub(super) use callables::*;
pub(super) use classes::*;
pub(super) use dynamic_references::*;
pub(super) use static_members::*;
