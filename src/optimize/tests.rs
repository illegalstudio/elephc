//! Purpose:
//! Integration-style unit fixtures for optimizer passes over hand-built ASTs.
//! Provides shared imports and submodules for fold, propagate, prune, DCE, effects, and normalization tests.
//!
//! Called from:
//! - `crate::optimize::tests` through Rust's test harness
//!
//! Key details:
//! - Tests assert AST rewrites directly, so spans and statement ordering are part of the expected behavior.

use super::*;
use crate::names::Name;
use crate::parser::ast::{ClassProperty, StaticReceiver, Visibility};
use crate::span::Span;

mod effects;
mod propagate;
mod fold;
mod prune;
mod dce;
mod control;
mod normalize;
