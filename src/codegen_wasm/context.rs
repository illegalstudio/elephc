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
//! - Ref-cell pointers (PHP `=&` aliases and `foreach &$item`) are carried in a
//!   dedicated i32 local per slot (`ref_cell_ptrs`), since WASM locals are not
//!   addressable linear memory and a slot's own `WasmRepr` is the value repr, not a
//!   pointer. Owner slots register for end-of-scope release in `ref_cell_owners`,
//!   drained by the `Return` epilogue (see `refcell`).

use std::collections::{HashMap, HashSet};

use super::values::WasmRepr;
use super::wat::{FuncBuilder, ValType};
use super::WasmError;
use crate::ir::{
    BlockId, DataId, Function, Immediate, LocalKind, LocalSlotId, Module, Op, ValueId,
};
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
    /// Local holding the catch-block index of the innermost armed `try` handler.
    ///
    /// The landing pad copies it into `state_local` to resume dispatch inside the
    /// catch block. Only functions that use exceptions declare it.
    pub(super) handler_local: String,
    /// Per-try-token save slot holding the handler that was armed on entry.
    pub(super) handler_saves: std::collections::HashMap<i64, String>,
    /// Per-function local holding this frame's baseline value of the global
    /// `$__concat_off` cursor, captured in the prologue. `ConcatReset` restores
    /// `$__concat_off` to this so statement-boundary resets free temporaries.
    pub(super) concat_base_local: String,
    /// Counter for generating unique temp local names (`$__tmp0`, `$__tmp1`, ...).
    pub(super) temp_counter: u32,
    /// String-literal layout indexed by `DataId.as_raw()`: `(byte_offset, byte_len)`
    /// of each interned string's data segment in linear memory.
    pub(super) str_literals: &'a [(u32, u32)],
    /// Class property STRING defaults, keyed by content rather than by `DataId`.
    ///
    /// Object construction writes defaults inline instead of calling the class's
    /// `_class_propinit_*` function, so a string default arrives as literal text
    /// with no `DataId` to address — see `objects::literal_default_strings`, which
    /// is also what guarantees every default reachable here has an entry.
    pub(super) default_strings: &'a HashMap<String, (u32, u32)>,
    /// Per-closure capture-tag-byte-array base address, indexed by the closure's
    /// position in `module.closures` (its `entry_index`). `0` for a no-capture
    /// closure (no tag array emitted). `ClosureNew` stamps this as the
    /// descriptor's `capture_tags_ptr` so the release runtime can walk it.
    pub(super) closure_tag_ptrs: &'a [u32],
    /// Distinct names of user FREE FUNCTIONS that are the target of an
    /// `Op::FirstClassCallableNew` somewhere in the module, in first-seen order
    /// (P7d2a). These occupy the UNIFIED callable-ladder index space *after* the
    /// closures: an FCC target's `entry_index` is `module.closures.len() + its
    /// position here`, so closures keep `0..N` and FCC entries take `N..N+M`.
    /// `lower_first_class_callable_new` looks a target name up here (via
    /// `fcc_entry_index`) to stamp the kind-6 descriptor's `entry_index`; a name
    /// absent from this slice is a builtin/extern/method FCC target (deferred) and
    /// is rejected rather than miscompiled. Built once in `generate()`.
    pub(super) fcc_entries: &'a [String],
    /// Compile-time placement of every static property, keyed by `"DeclaringClass::name"`.
    ///
    /// A static is ONE 16-byte slot in static memory for the whole program, so its address
    /// is a constant and `Op::LoadStaticProperty` / `Op::StoreStaticProperty` need no
    /// runtime lookup. Built once in `generate()`; see `codegen_wasm::statics`.
    pub(super) static_slots: &'a super::statics::StaticSlots,
    /// Maps an `IterStart` result `ValueId::as_raw()` to its iterator locals, so the
    /// loop's `IterNext`/`IterCurrent*` ops (which reference the iterator value by
    /// dominance) recover its source/cursor without any heap state.
    pub(super) iter_state: HashMap<u32, IterSlots>,
    /// Maps a slot raw id (php-visible OR owner) to the `$name` of the i32 local
    /// holding that slot's ref-cell pointer. Populated by `PromoteLocalRefCell`
    /// (both the php-visible and the owner slot share one local) and by
    /// `AliasLocalRefCell` (the target gets its own local, copied from the source).
    /// `LoadRefCell`/`StoreRefCell`/`ReleaseLocalRefCell` look the pointer up here;
    /// a missing entry means the slot is not ref-bound and is a lowering error.
    /// By-ref parameters and foreach bindings also populate it as borrowed
    /// provenance; capability checks prevent either binding from escaping.
    pub(super) ref_cell_ptrs: HashMap<u32, String>,
    /// Slots whose current ref-cell binding points at an owned kind-7 heap cell.
    ///
    /// Aliases inherit this bit from their source. By-ref parameters and foreach
    /// element bindings remain absent, so closure capture cannot make borrowed or
    /// interior storage escape.
    pub(super) owned_ref_cell_slots: HashSet<u32>,
    /// Owner slots needing end-of-scope release at every `Return`, paired with the
    /// payload `PhpType` (already `codegen_repr`-applied) that drives the release.
    /// Collected when lowering `PromoteLocalRefCell` or directly promoting a fresh
    /// closure capture. The `Return` epilogue releases each non-null owner; an
    /// explicit `ReleaseLocalRefCell` zeroes the owner first, so the epilogue skips
    /// it — idempotent, no double-free.
    pub(super) ref_cell_owners: Vec<(u32, PhpType)>,
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

    /// Resolves a class property string default's `(byte_offset, byte_len)` in linear memory.
    ///
    /// Returns `Err(WasmError::Unsupported)` if the content was never laid out, which can only
    /// happen if `plan_module` and the object-construction lowering disagree about which
    /// defaults are materializable — never for a module the capability audit admitted.
    pub(super) fn default_str_literal(&self, value: &str) -> Result<(u32, u32)> {
        self.default_strings.get(value).copied().ok_or_else(|| {
            WasmError::Unsupported(format!(
                "string property default {:?} has no data segment",
                value
            ))
        })
    }

    /// Resolves the capture-tag-byte-array base address for the closure whose
    /// `entry_index` is its position in `module.closures`. Returns `0` for a
    /// no-capture closure (no tag array emitted) or an out-of-range index
    /// (defensive; should not happen for a valid `ClosureNew`).
    pub(super) fn closure_tag_base(&self, entry_index: usize) -> u32 {
        self.closure_tag_ptrs.get(entry_index).copied().unwrap_or(0)
    }

    /// Resolves the unified callable-ladder `entry_index` for a first-class
    /// callable whose target is the user free function `name`, or `None` when the
    /// name is not a registered free-function FCC target (a builtin/extern/method
    /// target — the caller rejects those as a deferred slice rather than
    /// miscompiling). The index is `module.closures.len() + position`, so it never
    /// collides with a closure's index (`0..N`). Exact-match by name: the registry
    /// is built from the SAME interned `Immediate::Data` names these instructions
    /// carry, so the lookup string equals the registered string.
    pub(super) fn fcc_entry_index(&self, name: &str) -> Option<u32> {
        self.fcc_entries
            .iter()
            .position(|n| n == name)
            .map(|p| self.module.closures.len() as u32 + p as u32)
    }

    /// Declares a fresh temp local of the given type and returns its `$name` reference.
    ///
    /// Temp locals are named `$__tmp{N}` where N is `temp_counter` before increment.
    pub(super) fn fresh_temp(&mut self, ty: ValType) -> String {
        let name = format!("__tmp{}", self.temp_counter);
        self.temp_counter += 1;
        self.fb.local(&name, ty)
    }

    /// Declares and records the locals for an `IterStart` before block bodies are
    /// lowered.
    ///
    /// EIR block storage order does not guarantee that the block containing
    /// `IterStart` precedes the loop header containing `IterNext`. Reserving every
    /// iterator in a prepass makes lowering independent of that order.
    pub(super) fn iter_reserve(&mut self, iter: ValueId, elem: PhpType, is_hash: bool) {
        let n = self.temp_counter;
        self.temp_counter += 1;
        let source_local = self.fb.local(&format!("__iter_src{}", n), ValType::I32);
        let cursor_local = self.fb.local(&format!("__iter_cur{}", n), ValType::I64);
        self.iter_state.insert(
            iter.as_raw(),
            IterSlots {
                source: source_local,
                cursor: cursor_local,
                elem,
                is_hash,
            },
        );
    }

    /// Emits the runtime initialization for a previously reserved iterator.
    ///
    /// Captures the source pointer and seeds the cursor to `-1` for indexed
    /// arrays or `-2` for associative hashes. The instruction prepass guarantees
    /// that the iterator locals already exist even when its loop-header block is
    /// stored before the `IterStart` block.
    pub(super) fn iter_initialize(&mut self, iter: ValueId, source: ValueId) -> Result<()> {
        let slots = self.iter_slots(iter)?;
        let source_local = slots.source.clone();
        let cursor_local = slots.cursor.clone();
        let is_hash = slots.is_hash;
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
    /// the type-aware transfer layer.
    ///
    /// Every source is loaded into temp locals before any destination is stored,
    /// so the move remains safe even when a destination param is also a source
    /// arg (e.g. a loop block branching to itself).  Concrete-to-Mixed conversions
    /// are applied by the transfer helper rather than by matching component counts.
    pub(super) fn materialize_block_args(
        &mut self,
        target: BlockId,
        args: &[ValueId],
    ) -> Result<()> {
        super::transfer::emit_transfer_block_args(self, target, args)
    }

    /// Looks up the `$name` of the i32 local holding a slot's ref-cell pointer.
    ///
    /// Returns `Err(WasmError::Unsupported)` when the slot is not ref-bound — the
    /// caller surfaces this as a clean diagnostic rather than miscompiling a
    /// ref-cell op against a plain local slot.
    pub(super) fn ref_cell_ptr(&self, slot_raw: u32) -> Result<&str> {
        self.ref_cell_ptrs
            .get(&slot_raw)
            .map(String::as_str)
            .ok_or_else(|| {
                WasmError::Unsupported("ref-cell op on non-ref-bound slot".to_string())
            })
    }

    /// Registers the i32 local holding a slot's ref-cell pointer.
    ///
    /// Called by promotion (for php-visible and owner slots), aliases, by-ref
    /// parameters, and foreach ref bindings. A later registration for the same
    /// slot overwrites both its pointer mapping and owned/borrowed provenance.
    pub(super) fn register_ref_cell_ptr(
        &mut self,
        slot_raw: u32,
        local: String,
        owned: bool,
    ) {
        self.ref_cell_ptrs.insert(slot_raw, local);
        if owned {
            self.owned_ref_cell_slots.insert(slot_raw);
        } else {
            self.owned_ref_cell_slots.remove(&slot_raw);
        }
    }

    /// Returns whether a ref-bound slot points at a cell owned by this frame.
    ///
    /// Aliases inherit ownership provenance when registered. By-ref parameters and
    /// foreach element bindings do not, so escaping closure capture rejects them.
    pub(super) fn ref_cell_has_owner(&self, slot_raw: u32) -> bool {
        self.owned_ref_cell_slots.contains(&slot_raw)
    }

    /// Records an owner slot + payload type for the end-of-scope release epilogue.
    ///
    /// Called by EIR promotion and by direct closure-capture promotion. An aliased
    /// target adds no owner: the source frame's owner remains the sole frame-side
    /// releaser, while closure descriptors retain their own runtime references.
    pub(super) fn add_ref_cell_owner(&mut self, owner_raw: u32, payload_type: PhpType) {
        if !self.ref_cell_owners.iter().any(|(s, _)| *s == owner_raw) {
            self.ref_cell_owners.push((owner_raw, payload_type));
        }
    }

    /// Emits the release sequence for one ref-cell owner at function exit.
    ///
    /// `ptr_local` is the i32 local holding the cell pointer; `payload_type` is the
    /// value type stored in the cell (already `codegen_repr`-applied). The sequence
    /// calls the dedicated kind-7 release helper with a typed payload tag and then
    /// zeroes the owner local. The helper frees the payload and cell only when the
    /// final owner disappears.
    pub(super) fn emit_ref_cell_release(
        &mut self,
        ptr_local: &str,
        payload_type: &PhpType,
    ) -> Result<()> {
        super::refcell::emit_ref_cell_release_seq(self, ptr_local, payload_type)
    }

    /// Emits the owner-slot release epilogue at a function return.
    ///
    /// Iterates every recorded owner slot and drops one cell reference. Idempotent:
    /// an explicit `ReleaseLocalRefCell` earlier zeroed that owner, so the runtime
    /// release helper receives null and safely no-ops.
    /// Mirrors the native `emit_ref_cell_owner_epilogue_cleanup`.
    pub(super) fn emit_ref_cell_owner_epilogue(&mut self) -> Result<()> {
        let owners = self.ref_cell_owners.clone();
        for (owner_raw, payload_type) in owners {
            let ptr_local = self.ref_cell_ptr(owner_raw)?.to_string();
            super::refcell::emit_ref_cell_release_seq(self, &ptr_local, &payload_type)?;
        }
        Ok(())
    }

    /// Releases ordinary owned local slots at a function or main return.
    ///
    /// This mirrors the native frame epilogue: parameters, ref-cell-backed slots,
    /// non-owning local kinds, never-written slots, and the slot moved out as the
    /// return value are excluded. String pointers use the guarded string free;
    /// arrays, hashes, objects, Mixed cells, and other refcounted pointers use the
    /// kind dispatcher; callable descriptors are narrowed from their i64 ABI
    /// carrier. Locals are cleared before release so destructor re-entry cannot
    /// observe a stale owner in the current frame.
    pub(super) fn emit_local_epilogue_cleanup(
        &mut self,
        returned_slot: Option<LocalSlotId>,
    ) -> Result<()> {
        let returned_raw = returned_slot.map(LocalSlotId::as_raw);
        let candidates = self
            .function
            .locals
            .iter()
            .enumerate()
            .filter(|(index, _)| *index >= self.function.params.len())
            .filter(|(_, local)| {
                matches!(
                    local.kind,
                    LocalKind::PhpLocal
                        | LocalKind::HiddenTemp
                        | LocalKind::OwnedTemp
                        | LocalKind::NamedArgTemp
                )
            })
            .filter(|(index, local)| {
                let raw = *index as u32;
                if Some(raw) == returned_raw || self.ref_cell_ptrs.contains_key(&raw) {
                    return false;
                }
                let slot = LocalSlotId::from_raw(raw);
                let explicitly_stored = self.function.instructions.iter().any(|inst| {
                    inst.op == Op::StoreLocal
                        && matches!(
                            inst.immediate,
                            Some(Immediate::LocalSlot(candidate)) if candidate == slot
                        )
                });
                explicitly_stored
                    || (self.function.flags.is_main
                        && local.name.as_deref() == Some("argv"))
            })
            .filter_map(|(index, local)| {
                let php_type = local.php_type.codegen_repr();
                if matches!(php_type, PhpType::Str | PhpType::Callable)
                    || php_type.is_refcounted()
                {
                    Some((index as u32, php_type))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // An indexed-array PARAMETER is owned by the callee: the caller increfs before the call
        // and never releases, so this is what balances it. That is what gives PHP's by-value
        // semantics — a mutation inside sees two owners and copies on write, leaving the caller's
        // array alone; `__rt_array_ensure_unique` hands the callee the clone and drops the
        // original back to the caller's single reference, which this release must NOT touch.
        //
        // A returned parameter moves out instead, and a ref-bound one is the caller's cell.
        let mut candidates = candidates;
        for (index, local) in self.function.locals.iter().enumerate() {
            if index >= self.function.params.len() {
                break;
            }
            let raw = index as u32;
            if Some(raw) == returned_raw || self.ref_cell_ptrs.contains_key(&raw) {
                continue;
            }
            let php_type = local.php_type.codegen_repr();
            if matches!(php_type, PhpType::Array(_)) {
                candidates.push((raw, php_type));
            }
        }

        for (raw, php_type) in candidates {
            let repr = self.slot_repr(LocalSlotId::from_raw(raw))?.clone();
            match repr {
                WasmRepr::Ptr(local) => {
                    self.fb
                        .ins(&format!("local.get {}", local), "owned local pointer to release");
                    self.fb.ins("i32.const 0", "clear owned local before release");
                    self.fb.ins(&format!("local.set {}", local), "");
                    self.fb
                        .ins("call $__rt_decref_any", "release local value by runtime kind");
                }
                WasmRepr::Str { ptr, len } => {
                    self.fb
                        .ins(&format!("local.get {}", ptr), "owned local string to release");
                    self.fb.ins("i32.const 0", "clear owned string pointer");
                    self.fb.ins(&format!("local.set {}", ptr), "");
                    self.fb.ins("i64.const 0", "clear owned string length");
                    self.fb.ins(&format!("local.set {}", len), "");
                    self.fb
                        .ins("call $__rt_heap_free_safe", "free owned local string");
                }
                WasmRepr::I64(local) if php_type == PhpType::Callable => {
                    self.fb
                        .ins(&format!("local.get {}", local), "owned callable descriptor");
                    self.fb
                        .ins("i32.wrap_i64", "narrow callable descriptor to i32");
                    self.fb.ins("i64.const 0", "clear callable local");
                    self.fb.ins(&format!("local.set {}", local), "");
                    self.fb
                        .ins("call $__rt_decref_any", "release callable descriptor");
                }
                WasmRepr::I64(_)
                | WasmRepr::F64(_)
                | WasmRepr::Tagged { .. }
                | WasmRepr::Void => {}
            }
        }
        Ok(())
    }

    /// Emits the by-value closure-capture release epilogue at a function return.
    ///
    /// Each slot the body reassigned (recorded in `Function::reassigned_capture_slots`)
    /// owns its stored value rather than borrowing the descriptor's, so it is released
    /// here before the function returns — the WASM analogue of the native
    /// `reassigned_capture_epilogue_locals`. `returned_slot` (the slot whose `LoadLocal`
    /// provides this return's value, if any) is skipped: the return moves that value
    /// out, so releasing it would double-free. Refcounted pointers go through the
    /// null/bounds-guarded `__rt_decref_any`; a string is freed via `__rt_heap_free_safe`;
    /// a callable narrows its i64 descriptor first. The slot local is zeroed after
    /// release so a later read cannot observe a dangling pointer. No-op for non-closures
    /// (the set is empty).
    pub(super) fn emit_reassigned_capture_epilogue(
        &mut self,
        returned_slot: Option<LocalSlotId>,
    ) -> Result<()> {
        if self.function.reassigned_capture_slots.is_empty() {
            return Ok(());
        }
        let returned_raw = returned_slot.map(|s| s.as_raw());
        let mut slots: Vec<u32> = self
            .function
            .reassigned_capture_slots
            .iter()
            .map(|s| s.as_raw())
            .collect();
        slots.sort_unstable();
        for raw in slots {
            if Some(raw) == returned_raw || self.ref_cell_ptrs.contains_key(&raw) {
                continue;
            }
            let slot = LocalSlotId::from_raw(raw);
            let php_type = self.function.locals[raw as usize].php_type.codegen_repr();
            let repr = self.slot_repr(slot)?.clone();
            match repr {
                WasmRepr::Ptr(local) => {
                    self.fb
                        .ins(&format!("local.get {}", local), "reassigned capture pointer to release");
                    self.fb
                        .ins("call $__rt_decref_any", "release the owned reassigned capture by kind");
                    self.fb.ins("i32.const 0", "clear the released capture slot");
                    self.fb.ins(&format!("local.set {}", local), "");
                }
                WasmRepr::Str { ptr, .. } => {
                    self.fb
                        .ins(&format!("local.get {}", ptr), "reassigned capture string to free");
                    self.fb
                        .ins("call $__rt_heap_free_safe", "free the owned reassigned capture string");
                    self.fb.ins("i32.const 0", "clear the released capture slot");
                    self.fb.ins(&format!("local.set {}", ptr), "");
                }
                WasmRepr::I64(local) if php_type == PhpType::Callable => {
                    self.fb
                        .ins(&format!("local.get {}", local), "reassigned callable capture descriptor");
                    self.fb
                        .ins("i32.wrap_i64", "narrow the callable descriptor pointer to i32");
                    self.fb
                        .ins("call $__rt_decref_any", "release the callable descriptor (kind 6)");
                    self.fb.ins("i64.const 0", "clear the released capture slot");
                    self.fb.ins(&format!("local.set {}", local), "");
                }
                WasmRepr::I64(_) | WasmRepr::F64(_) | WasmRepr::Void => {}
                WasmRepr::Tagged { .. } => {
                    return Err(WasmError::Unsupported(
                        "release of a reassigned Mixed closure capture on wasm32-wasi".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}
