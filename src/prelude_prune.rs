//! Purpose:
//! Hosts the pre-name-resolution usage scan the class gates and superglobal materialization read.
//!
//! Called from:
//! - `crate::types::checker::builtin_class_gate`, which opens a class hierarchy only for a
//!   program that can reach it.
//! - `crate::superglobals`, which materializes a superglobal because the program NAMES it.
//! - `crate::web_prelude`, for the same reachability question about its callable session handler.
//!
//! Key details:
//! - THIS MODULE USED TO PRUNE PRELUDES, and no longer does. Injecting the image surface into a
//!   program calling two GD functions turned a 9,501-line assembly file into a 162,630-line one,
//!   and a local reachability walk over the prelude cut that back. The walk was WRONG about one
//!   thing, and it mattered: it harvested LITERAL names, so `$fn = 'image' . 'colorallocate';
//!   $fn($im, …)` had its target pruned and the program died with `Call to undefined function`
//!   where PHP answers.
//! - The global declaration reachability pass already answers the same question and treats an
//!   unknown `$fn()` conservatively, so the preludes now record their COMPLETE selected surface
//!   in the inventory and let that pass decide. One pruner, and the one that handles the hazard.
//! - What remains here is the SCAN, which never removed anything on its own.

pub(crate) mod usage;
mod callable_names;
