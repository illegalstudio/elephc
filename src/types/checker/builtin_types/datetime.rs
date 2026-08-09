//! Purpose:
//! Facade for synthetic checker metadata covering PHP date/time classes and helpers.
//! Focused modules own timezone behavior, DateTime factories/setters, procedural
//! adapters, intervals, and final class-map injection.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`.
//! - `crate::types::checker::driver` initialization.
//!
//! Key details:
//! - Synthetic methods remain ordinary PHP AST lowered by the normal pipeline.
//! - Timezone bridge-dependent methods stay gated during declaration injection.

use std::collections::HashMap;

use crate::names::Name;
use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt, StmtKind,
    TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::calendar;
use super::declarations::InterfaceDeclInfo;
use super::timezone_ids;

mod ast;
mod basics;
mod create_from_format;
mod factories;
mod gate;
mod injection;
mod interface;
mod interval_constructor;
mod interval_diff;
mod interval_factory;
mod interval_format;
mod parse_formats;
mod parse_misc;
mod procedural_methods;
mod setter_helpers;
mod setters;
mod strftime;
mod strptime;
mod sun_sources;
mod timezone;

#[allow(unused_imports)]
use ast::*;
#[allow(unused_imports)]
use basics::*;
#[allow(unused_imports)]
use create_from_format::*;
#[allow(unused_imports)]
use factories::*;
#[allow(unused_imports)]
use interface::*;
#[allow(unused_imports)]
use interval_constructor::*;
#[allow(unused_imports)]
use interval_diff::*;
#[allow(unused_imports)]
use interval_factory::*;
#[allow(unused_imports)]
use interval_format::*;
#[allow(unused_imports)]
use parse_formats::*;
#[allow(unused_imports)]
use parse_misc::*;
#[allow(unused_imports)]
use procedural_methods::*;
#[allow(unused_imports)]
use setter_helpers::*;
#[allow(unused_imports)]
use setters::*;
#[allow(unused_imports)]
use strftime::*;
#[allow(unused_imports)]
use strptime::*;
#[allow(unused_imports)]
use sun_sources::*;
#[allow(unused_imports)]
use timezone::*;

pub(crate) use gate::program_may_reference_datetime;
pub(crate) use injection::inject_builtin_datetime;
