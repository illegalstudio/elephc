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
use crate::types::PhpType;

/// The WebAssembly locals backing one `foreach` iterator.
///
/// WebAssembly has no addressable machine stack, so an iterator's state lives in
/// per-function locals (private to each invocation, so recursion is safe and no
/// teardown is needed): a `source` pointer and a signed `cursor`. `elem` is the
/// element PHP type, used to pick the element getter and whether the current value
/// must be boxed into a Mixed cell.
///
/// For an indexed ARRAY the cursor is the element index (starts at -1, pre-incremented
/// to 0). For an associative HASH the cursor is the current entry's slot index (starts
/// at the `-2` "before first" sentinel, advanced by `__rt_hash_iter_next`); `is_hash`
/// selects between the two lowering paths.
pub(super) struct IterSlots {
    /// `$name` of the i32 local holding the source array/hash pointer.
    pub(super) source: String,
    /// `$name` of the i64 local holding the current cursor (array index, or hash slot index).
    pub(super) cursor: String,
    /// The element type (its `codegen_repr`): an array's element type, or a hash's value type.
    pub(super) elem: PhpType,
    /// Whether the source is an associative hash (vs an indexed array).
    pub(super) is_hash: bool,
}

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
    /// Per-closure capture-tag-byte-array base address, indexed by the closure's
    /// position in `module.closures` (its `entry_index`). `0` for a no-capture
    /// closure (no tag array emitted). `ClosureNew` stamps this as the
    /// descriptor's `capture_tags_ptr` so the release runtime can walk it.
    pub(super) closure_tag_ptrs: &'a [u32],
    /// Maps an `IterStart` result `ValueId::as_raw()` to its iterator locals, so the
    /// loop's `IterNext`/`IterCurrent*` ops (which reference the iterator value by
    /// dominance) recover its source/cursor without any heap state.
    pub(super) iter_state: HashMap<u32, IterSlots>,
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

    /// Returns the `PhpType` carried by an EIR value (read from the function's
    /// value table).
    ///
    /// Used by method-call lowering to inspect the receiver's declared type and
    /// resolve the target class's vtable information.
    pub(super) fn value_php_type(&self, v: ValueId) -> Result<PhpType> {
        self.function
            .value(v)
            .map(|val| val.php_type.clone())
            .ok_or_else(|| WasmError::Unsupported(format!("value {:?} has no php_type", v)))
    }

    /// Emits `local.get` for each local backing a local slot, in canonical order.
    ///
    /// Used by static-method lowering's lexical fallback to forward the current
    /// `this` (slot 0) as the receiver of an instance method call (e.g.
    /// `parent::__construct()` chaining).
    pub(super) fn emit_load_slot(&mut self, s: LocalSlotId) -> Result<()> {
        let refs = self.slot_repr(s)?.local_refs();
        for local_ref in refs {
            self.fb
                .ins(&format!("local.get {}", local_ref), "load slot component");
        }
        Ok(())
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

    /// Resolves the capture-tag-byte-array base address for the closure whose
    /// `entry_index` is its position in `module.closures`. Returns `0` for a
    /// no-capture closure (no tag array emitted) or an out-of-range index
    /// (defensive; should not happen for a valid `ClosureNew`).
    pub(super) fn closure_tag_base(&self, entry_index: usize) -> u32 {
        self.closure_tag_ptrs.get(entry_index).copied().unwrap_or(0)
    }

    /// Declares a fresh temp local of the given type and returns its `$name` reference.
    ///
    /// Temp locals are named `$__tmp{N}` where N is `temp_counter` before increment.
    pub(super) fn fresh_temp(&mut self, ty: ValType) -> String {
        let name = format!("__tmp{}", self.temp_counter);
        self.temp_counter += 1;
        self.fb.local(&name, ty)
    }

    /// Declares the iterator locals for an `IterStart`, emits the initialization
    /// (capture the source pointer, set the cursor to its start sentinel), and records
    /// them under the iterator value's id.
    ///
    /// `source` must already have a `WasmRepr` (a single i32 pointer for an array or a
    /// hash); `elem` is the array's element type or the hash's value type. The cursor is
    /// seeded to `-1` for an indexed array (pre-incremented to 0 by `IterNext`) or to the
    /// `-2` "before first" sentinel for a hash (`__rt_hash_iter_next` maps it to the list
    /// head). The iterator result value's own local is left untouched — downstream ops
    /// look the iterator up by id, not by its repr.
    pub(super) fn iter_declare(
        &mut self,
        iter: ValueId,
        source: ValueId,
        elem: PhpType,
        is_hash: bool,
    ) -> Result<()> {
        let n = self.temp_counter;
        self.temp_counter += 1;
        let source_local = self.fb.local(&format!("__iter_src{}", n), ValType::I32);
        let cursor_local = self.fb.local(&format!("__iter_cur{}", n), ValType::I64);
        self.emit_load_value(source)?;
        self.fb
            .ins(&format!("local.set {}", source_local), "iterator source pointer");
        if is_hash {
            self.fb
                .ins("i64.const -2", "hash cursor (before-first sentinel)");
        } else {
            self.fb.ins("i64.const -1", "indexed cursor (pre-increment to 0)");
        }
        self.fb
            .ins(&format!("local.set {}", cursor_local), "init iterator cursor");
        self.iter_state.insert(
            iter.as_raw(),
            IterSlots {
                source: source_local,
                cursor: cursor_local,
                elem,
                is_hash,
            },
        );
        Ok(())
    }

    /// Looks up the iterator locals for an `IterStart` result value.
    pub(super) fn iter_slots(&self, iter: ValueId) -> Result<&IterSlots> {
        self.iter_state
            .get(&iter.as_raw())
            .ok_or_else(|| WasmError::Unsupported(format!("iterator {:?} has no state", iter)))
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
