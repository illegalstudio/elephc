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
