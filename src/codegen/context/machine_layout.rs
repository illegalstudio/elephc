//! Purpose:
//! Per-function machine-layout state: the block emitted next, the compare fused into the
//! current block's conditional branch, and cold paths deferred to the end of the function.
//!
//! Called from:
//! - `crate::codegen::block_emit` (layout order, compare fusion, cold flush).
//! - `crate::codegen::lower_term` and checked-arithmetic lowerers (fallthrough and cold paths).
//!
//! Key details:
//! - Cold text is only deferred while `block_emit` has opened a cold section for the function;
//!   other emitters get the same code inline behind a jump, so nothing is ever lost.
//! - Deferred text is appended after the last block, where no block can fall into it, and
//!   starts with its instruction's `@src` marker so line info stays with its PHP statement.

use crate::codegen::abi;
use crate::codegen::Result;
use crate::ir::{BlockId, InstId};

use super::FunctionContext;

/// Machine-layout facts for the block currently being emitted.
#[derive(Debug, Default)]
pub(crate) struct MachineLayoutState {
    /// Block emitted immediately after the current one, if any.
    pub(crate) next_block: Option<BlockId>,
    /// Integer compare whose flags the current block's `cond_br` consumes directly.
    pub(crate) fused_compare: Option<InstId>,
    /// Out-of-line cold text collected for this function, when deferral is active.
    cold_text: Option<String>,
}

impl FunctionContext<'_> {
    /// Returns the label of the block that will be emitted next, when one exists.
    pub(in crate::codegen) fn fallthrough_label(&self) -> Option<String> {
        self.machine_layout
            .next_block
            .and_then(|block| self.block_label_for_id(block).ok())
    }

    /// Returns true when `inst` is an integer compare lowered by the following `cond_br`.
    pub(in crate::codegen) fn is_fused_compare(&self, inst: InstId) -> bool {
        self.machine_layout.fused_compare == Some(inst)
    }

    /// Starts collecting cold paths for this function's out-of-line tail.
    pub(in crate::codegen) fn begin_cold_section(&mut self) {
        self.machine_layout.cold_text = Some(String::new());
    }

    /// Appends every deferred cold path after the function's last block and stops deferring.
    pub(in crate::codegen) fn flush_cold_section(&mut self) {
        let Some(text) = self.machine_layout.cold_text.take() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.emitter.comment("-- cold paths kept out of the hot fallthrough --");
        self.emitter.append_text(&text);
    }

    /// Emits a cold path that always leaves by an explicit jump or a non-returning call.
    ///
    /// With an open cold section the path lands after the function's last block, so the hot
    /// path's conditional branch falls through on the common case. Otherwise it is emitted
    /// inline behind an unconditional jump, preserving the same control flow.
    pub(in crate::codegen) fn emit_cold_path(
        &mut self,
        emit: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        if self.machine_layout.cold_text.is_some() {
            let scope = self.emitter.begin_out_of_line();
            // Repeat the originating instruction's source marker so debug line info and source
            // maps attribute the out-of-line code to its PHP statement, not the last block.
            let marker = match self.current_inst {
                Some(inst) => crate::codegen::block_emit::emit_instruction_source_marker(self, inst),
                None => Ok(()),
            };
            // Close the detached scope even on error, so the caller's text is restored.
            let result = marker.and_then(|()| emit(self));
            let body = self.emitter.end_detached(scope);
            if let Some(cold) = self.machine_layout.cold_text.as_mut() {
                cold.push_str(&body);
            }
            return result;
        }
        let skip = self.next_label("cold_path_skip");
        abi::emit_jump(self.emitter, &skip);
        emit(self)?;
        self.emitter.label(&skip);
        Ok(())
    }
}
