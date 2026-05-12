//! Purpose:
//! Target-aware assembly emission for generator wrappers and resume functions.
//! Top-level orchestrator that owns shared frame-slot helpers and the resume
//! function's mutable context, and re-exports the entry points consumed by
//! the rest of the generator pipeline.
//!
//! Called from:
//!  - `crate::codegen::functions::generator::mod::emit_generator_function()`.
//!
//! Key details:
//!  - The emit pass is split by responsibility:
//!    - `wrapper` — `_fn_<f>` wrapper symbol (allocates and stamps the frame).
//!    - `dispatcher` — `_fn_<f>__resume` state-machine prologue/epilogue and
//!      resume-label dispatch table.
//!    - `stmts` — body statements, branches, loops, and switch lowering.
//!    - `values` — expression lowering, Mixed-cell boxing, and the
//!      refcount-replace pattern for boxed slots.
//!    - `yields` — yield/yield-from suspension and `Generator::send` resume
//!      handling.
//!  - Frame slot offsets and the fixed-header layout live in
//!    `crate::codegen::runtime::generators::frame`; both wrapper and resume
//!    paths must agree with that contract.

use crate::codegen::runtime::generators::frame as gen_frame;
use crate::codegen::{emit::Emitter, platform::Arch};

mod dispatcher;
mod stmts;
mod values;
mod wrapper;
mod yields;

pub(in crate::codegen::functions::generator) use dispatcher::emit_resume;
pub(in crate::codegen::functions::generator) use wrapper::emit_wrapper;

const OFF_PARAMS_BASE: usize = gen_frame::FIXED_HEADER_BYTES;
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub(super) fn preserved_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x20",
        Arch::X86_64 => "r13",
    }
}

pub(super) fn slot_offset(idx: usize) -> usize {
    OFF_PARAMS_BASE + idx * 8
}

pub(super) fn aligned_frame_size_with_slots(slot_count: usize) -> usize {
    gen_frame::aligned_frame_size(slot_count * 8)
}

pub(super) struct LoopLabels {
    pub(super) end: String,
    pub(super) cont: String,
}

pub(super) struct ResumeCtx<'a> {
    pub(super) label: &'a str,
    pub(super) term_label: String,
    pub(super) end_label: String,
    pub(super) next_label_id: u32,
    pub(super) loop_stack: Vec<LoopLabels>,
}

impl<'a> ResumeCtx<'a> {
    pub(super) fn fresh_label(&mut self, hint: &str) -> String {
        let id = self.next_label_id;
        self.next_label_id += 1;
        format!("{}_{}_{}", self.label, hint, id)
    }
}
