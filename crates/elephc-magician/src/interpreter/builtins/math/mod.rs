//! Purpose:
//! Per-builtin eval registry entries and implementations for numeric functions.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!` and own their
//!   PHP-visible direct/by-value wrappers.

mod abs;
mod acos;
mod asin;
mod atan;
mod atan2;
mod ceil;
mod clamp;
mod cos;
mod cosh;
mod deg2rad;
mod exp;
mod fdiv;
mod floor;
mod fmod;
mod hypot;
mod intdiv;
mod log;
mod log10;
mod log2;
mod max;
mod min;
mod mt_rand;
mod pi;
mod pow;
mod rand;
mod random_bytes;
mod random_int;
mod rad2deg;
mod round;
mod sin;
mod sinh;
mod sqrt;
mod tan;
mod tanh;

pub(in crate::interpreter) use abs::*;
pub(in crate::interpreter) use acos::*;
pub(in crate::interpreter) use asin::*;
pub(in crate::interpreter) use atan::*;
pub(in crate::interpreter) use atan2::*;
pub(in crate::interpreter) use ceil::*;
pub(in crate::interpreter) use clamp::*;
pub(in crate::interpreter) use cos::*;
pub(in crate::interpreter) use cosh::*;
pub(in crate::interpreter) use deg2rad::*;
pub(in crate::interpreter) use exp::*;
pub(in crate::interpreter) use fdiv::*;
pub(in crate::interpreter) use floor::*;
pub(in crate::interpreter) use fmod::*;
pub(in crate::interpreter) use hypot::*;
pub(in crate::interpreter) use intdiv::*;
pub(in crate::interpreter) use log::*;
pub(in crate::interpreter) use log10::*;
pub(in crate::interpreter) use log2::*;
pub(in crate::interpreter) use max::*;
pub(in crate::interpreter) use min::*;
pub(in crate::interpreter) use mt_rand::*;
pub(in crate::interpreter) use pi::*;
pub(in crate::interpreter) use pow::*;
pub(in crate::interpreter) use rad2deg::*;
pub(in crate::interpreter) use rand::*;
pub(in crate::interpreter) use random_bytes::*;
pub(in crate::interpreter) use random_int::*;
pub(in crate::interpreter) use round::*;
pub(in crate::interpreter) use sin::*;
pub(in crate::interpreter) use sinh::*;
pub(in crate::interpreter) use sqrt::*;
pub(in crate::interpreter) use tan::*;
pub(in crate::interpreter) use tanh::*;
