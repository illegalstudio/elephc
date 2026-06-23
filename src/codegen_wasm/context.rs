//! Purpose:
//! Per-function lowering context shared by the wasm32-wasi control-flow lowering
//! (`function`) and the instruction lowering (`inst`). Owns the `FuncBuilder`,
//! the EIR-value/local-slot -> WASM-local maps, and the value load/store helpers.
//!
//! Called from:
//! - `crate::codegen_wasm::function::lower_function` constructs it; `function` and
//!   `inst` drive it while emitting a function body.
//!
//! Key details:
//! - Values and local slots are realized as WASM locals via `crate::codegen_wasm::values`.
//!   Loading a value pushes its local(s) onto the WASM operand stack in canonical
//!   order; storing pops them back in reverse, which keeps multi-local values
//!   (Str = ptr+len, Tagged = payload+tag) consistent.

use std::collections::HashMap;

use super::values::WasmRepr;
use super::wat::{FuncBuilder, ValType};
use super::WasmError;
use crate::ir::{BlockId, DataId, Function, LocalSlotId, Module, ValueId};

/// Result type for the lowering modules, using the parent module's `WasmError`.
pub(super) type Result<T> = std::result::Result<T, WasmError>;

/// Returns the WAT function symbol (without leading `$`) for a PHP function name.
///
/// Every character outside `[A-Za-z0-9_]` is replaced with `_` and the result is
/// prefixed with `fn_`. Function definitions (`function::lower_function`) and call
/// sites (`inst::lower_call`) MUST use this single helper so a `call $fn_<name>`
/// always matches the defined function's name.
pub(super) fn wasm_fn_symbol(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    format!("fn_{}", sanitized)
}

/// Context for lowering a single EIR function to WebAssembly.
///
/// Holds references to the module and function being lowered, the `FuncBuilder`
/// being emitted into, and maps from EIR IDs to their WebAssembly representations.
pub(super) struct FnCtx<'a> {
    /// The parent module (data pool for call-name resolution, cross-function references).
    pub(super) module: &'a Module,
    /// The function being lowered.
    pub(super) function: &'a Function,
    /// The WAT function builder.
    pub(super) fb: FuncBuilder,
    /// Maps `ValueId::as_raw()` to the `WasmRepr` of the SSA value's local(s).
    pub(super) value_locals: HashMap<u32, WasmRepr>,
    /// Maps `LocalSlotId::as_raw()` to the `WasmRepr` of the local slot's local(s).
    pub(super) slot_locals: HashMap<u32, WasmRepr>,
    /// The `$__state` local holding the current block index for dispatch.
    pub(super) state_local: String,
    /// Per-function local holding this frame's baseline value of the global
    /// `$__concat_off` cursor, captured in the prologue. `ConcatReset` restores
    /// `$__concat_off` to this so statement-boundary resets free temporaries.
    pub(super) concat_base_local: String,
    /// Counter for generating unique temp local names (`$__tmp0`, `$__tmp1`, ...).
    pub(super) temp_counter: u32,
    /// String-literal layout indexed by `DataId.as_raw()`: `(byte_offset, byte_len)`
    /// of each interned string's data segment in linear memory.
    pub(super) str_literals: &'a [(u32, u32)],
}

impl<'a> FnCtx<'a> {
    /// Looks up the `WasmRepr` for an SSA value.
    ///
    /// Returns `Ok(&WasmRepr)` if found, or `Err(WasmError::Unsupported)` if the
    /// value has no corresponding local (should not happen for valid EIR).
    pub(super) fn value_repr(&self, v: ValueId) -> Result<&WasmRepr> {
        self.value_locals
            .get(&v.as_raw())
            .ok_or_else(|| WasmError::Unsupported(format!("value {:?} has no repr", v)))
    }

    /// Looks up the `WasmRepr` for a local slot.
    ///
    /// Returns `Ok(&WasmRepr)` if found, or `Err(WasmError::Unsupported)` if the
    /// slot has no corresponding local (should not happen for valid EIR).
    #[allow(dead_code)]
    pub(super) fn slot_repr(&self, s: LocalSlotId) -> Result<&WasmRepr> {
        self.slot_locals
            .get(&s.as_raw())
            .ok_or_else(|| WasmError::Unsupported(format!("slot {:?} has no repr", s)))
    }

    /// Returns the block index for a `BlockId`.
    ///
    /// Block indices are exactly their raw IDs; this is a convention of the
    /// dispatch loop encoding.
    pub(super) fn block_index(&self, b: BlockId) -> u32 {
        b.as_raw()
    }

    /// Resolves a string literal's `(byte_offset, byte_len)` in linear memory.
    ///
    /// Returns `Err(WasmError::Unsupported)` if the `DataId` is out of range for
    /// the module's string-literal layout.
    pub(super) fn str_literal(&self, data_id: DataId) -> Result<(u32, u32)> {
        self.str_literals
            .get(data_id.as_raw() as usize)
            .copied()
            .ok_or_else(|| WasmError::Unsupported(format!("unknown string literal {:?}", data_id)))
    }

    /// Declares a fresh temp local of the given type and returns its `$name` reference.
    ///
    /// Temp locals are named `$__tmp{N}` where N is `temp_counter` before increment.
    pub(super) fn fresh_temp(&mut self, ty: ValType) -> String {
        let name = format!("__tmp{}", self.temp_counter);
        self.temp_counter += 1;
        self.fb.local(&name, ty)
    }

    /// Emits `local.get` for each local in the value's `WasmRepr`, in canonical order.
    ///
    /// For `I64`/`F64`/`Ptr`: pushes one value.
    /// For `Str`: pushes ptr then len.
    /// For `Tagged`: pushes payload then tag.
    /// For `Void`: pushes nothing.
    pub(super) fn emit_load_value(&mut self, v: ValueId) -> Result<()> {
        let repr = self.value_repr(v)?.clone();
        for local_ref in repr.local_refs() {
            self.fb
                .ins(&format!("local.get {}", local_ref), "load value component");
        }
        Ok(())
    }

    /// Pops the value's local(s) off the WASM operand stack into its locals.
    ///
    /// The operand stack must hold the value's components in canonical order
    /// (the order `emit_load_value` pushes them); this stores them back by setting
    /// each local in reverse, since `local.set` pops from the top of the stack.
    pub(super) fn emit_store_value(&mut self, v: ValueId) -> Result<()> {
        let repr = self.value_repr(v)?.clone();
        for local_ref in repr.local_refs().iter().rev() {
            self.fb
                .ins(&format!("local.set {}", local_ref), "store value component");
        }
        Ok(())
    }

    /// Emits code to push an `i32` truthiness value (1 or 0) for the given value.
    ///
    /// The value must have `WasmRepr::I64`; emits `local.get`, `i64.const 0`, `i64.ne`.
    /// Returns `Unsupported` for any other representation.
    pub(super) fn emit_truthy_i32(&mut self, v: ValueId) -> Result<()> {
        let repr = self.value_repr(v)?;
        match repr {
            WasmRepr::I64(local_ref) => {
                self.fb
                    .ins(&format!("local.get {}", local_ref), "load cond value");
                self.fb.ins("i64.const 0", "zero for comparison");
                self.fb.ins("i64.ne", "cond != 0 -> i32 truthy");
                Ok(())
            }
            _ => Err(WasmError::Unsupported(format!(
                "cond of non-i64 type: {:?}",
                repr
            ))),
        }
    }

    /// Copies branch arguments into the target block's parameter locals using
    /// parallel-move-safe ordering.
    ///
    /// Builds the flat source-local and dest-local lists, emits every `local.get`
    /// (forward order) before every `local.set` (reverse order). Because all gets
    /// precede all sets, this is safe even when a destination param is also a
    /// source arg (e.g. a loop block branching to itself).
    pub(super) fn materialize_block_args(
        &mut self,
        target: BlockId,
        args: &[ValueId],
    ) -> Result<()> {
        let target_block = self
            .function
            .block(target)
            .ok_or_else(|| WasmError::Unsupported(format!("target block {:?} not found", target)))?;

        let params = &target_block.params;
        if args.len() != params.len() {
            return Err(WasmError::Unsupported(format!(
                "branch arg count {} != param count {}",
                args.len(),
                params.len()
            )));
        }

        let mut src_refs: Vec<String> = Vec::new();
        for arg in args {
            let repr = self.value_repr(*arg)?.clone();
            src_refs.extend(repr.local_refs());
        }

        let mut dest_refs: Vec<String> = Vec::new();
        for param in params {
            let repr = self.value_repr(*param)?.clone();
            dest_refs.extend(repr.local_refs());
        }

        if src_refs.len() != dest_refs.len() {
            return Err(WasmError::Unsupported(format!(
                "source refs {} != dest refs {}",
                src_refs.len(),
                dest_refs.len()
            )));
        }

        if src_refs.is_empty() {
            return Ok(());
        }

        for src in &src_refs {
            self.fb
                .ins(&format!("local.get {}", src), "branch arg component");
        }
        for dest in dest_refs.iter().rev() {
            self.fb
                .ins(&format!("local.set {}", dest), "store param component");
        }

        Ok(())
    }
}
