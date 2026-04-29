use crate::support::*;

#[path = "optimizer/constant_folding.rs"]
mod constant_folding;
#[path = "optimizer/constant_propagation.rs"]
mod constant_propagation;
#[path = "optimizer/dead_code_elimination.rs"]
mod dead_code_elimination;
