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
    BlockId, DataId, Function, Immediate, InstId, LocalKind, LocalSlotId, Module, Op, ValueId,
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
    /// `$name` of the i32 local holding the RUNTIME container kind, when the source is a boxed
    /// value whose storage only the tag decides: 0 indexed, 1 hash, 2 not iterable.
    ///
    /// `None` for a source whose EIR type already settles it, which keeps every statically typed
    /// `foreach` on exactly the code it had before.
    pub(super) dynamic_kind: Option<String>,
    /// Whether a dynamic source is a raw CONTAINER POINTER (an `iterable` parameter) rather
    /// than a boxed cell. Both are dynamic — only the heap header versus the cell tag answers
    /// which storage it is — so they share every helper except the one that opens the walk.
    pub(super) dynamic_from_pointer: bool,
    /// Whether the source is a GENERATOR, whose walk is a resume rather than a cursor.
    ///
    /// A generator has no container to index: `source` holds its frame, advancing means
    /// calling `__rt_gen_resume`, and the current value is the one the frame recorded at
    /// the yield. `cursor` is unused, so nothing seeds it.
    pub(super) is_generator: bool,
}

/// Result type for the lowering modules, using the parent module's `WasmError`.
pub(super) type Result<T> = std::result::Result<T, WasmError>;

/// Per-function facts about how SSA values and local slots are defined and used.
///
/// Every field answers exactly the question a full scan of `Function::instructions` used to
/// answer, so a query through the index is interchangeable with the scan it replaces.
pub(super) struct UsageIndex {
    /// Values that some `Op::Release` takes as an operand.
    pub(super) released: HashSet<ValueId>,
    /// Values that some instruction other than `Op::Release` takes as an operand.
    pub(super) used_beyond_release: HashSet<ValueId>,
    /// For every value, the `result` of each USE that takes it as an operand.
    ///
    /// An instruction operand records the instruction's `result`. A TERMINATOR operand
    /// (`return`, `throw`, branch and switch arguments, the condition, a suspend's key/value)
    /// records `None`, which no instruction result equals: a value a terminator still reads
    /// is therefore never "single use" for a caller about to free it after its one
    /// instruction use.
    pub(super) user_results: HashMap<ValueId, Vec<Option<ValueId>>>,
    /// Values produced by an `Op::Acquire`.
    pub(super) acquired: HashSet<ValueId>,
    /// Index into `Function::instructions` of the first instruction that produces each value.
    pub(super) first_definition: HashMap<ValueId, usize>,
    /// Raw ids of the slots some `Op::StoreLocal` writes.
    pub(super) stored_slots: HashSet<u32>,
    /// Results of the slot loads that are a MOVE: the next non-`Nop` instruction in the same
    /// block is an `Op::UnsetLocal` of the slot the load reads.
    pub(super) moved_slot_loads: HashSet<ValueId>,
}

impl UsageIndex {
    /// Builds the index with one pass over the function's instructions.
    fn build(function: &Function) -> Self {
        let mut index = UsageIndex {
            released: HashSet::new(),
            used_beyond_release: HashSet::new(),
            user_results: HashMap::new(),
            acquired: HashSet::new(),
            first_definition: HashMap::new(),
            stored_slots: HashSet::new(),
            moved_slot_loads: HashSet::new(),
        };
        for (position, inst) in function.instructions.iter().enumerate() {
            for operand in &inst.operands {
                if inst.op == Op::Release {
                    index.released.insert(*operand);
                } else {
                    index.used_beyond_release.insert(*operand);
                }
                // A repeated operand adds a duplicate entry, which no query can tell apart.
                index
                    .user_results
                    .entry(*operand)
                    .or_default()
                    .push(inst.result);
            }
            if let Some(result) = inst.result {
                index.first_definition.entry(result).or_insert(position);
                if inst.op == Op::Acquire {
                    index.acquired.insert(result);
                }
            }
            if inst.op == Op::StoreLocal {
                if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                    index.stored_slots.insert(slot.as_raw());
                }
            }
        }
        for block in &function.blocks {
            if let Some(terminator) = &block.terminator {
                for operand in terminator_operands(terminator) {
                    index.user_results.entry(operand).or_default().push(None);
                }
            }
        }
        index.collect_moved_slot_loads(function);
        index
    }

    /// Fills `moved_slot_loads` with one walk over each block.
    ///
    /// A load qualifies when it is the first definition of its result, carries a slot
    /// immediate, and the next non-`Nop` instruction after it in its block unsets that slot.
    fn collect_moved_slot_loads(&mut self, function: &Function) {
        for block in &function.blocks {
            let ids: Vec<_> = block
                .instructions
                .iter()
                .filter_map(|id| function.instruction(*id).map(|inst| (id.as_raw() as usize, inst)))
                .collect();
            for (at, (position, load)) in ids.iter().enumerate() {
                let Some(result) = load.result else { continue };
                if self.first_definition.get(&result) != Some(position) {
                    continue;
                }
                let Some(Immediate::LocalSlot(slot)) = load.immediate else {
                    continue;
                };
                let next = ids[at + 1..].iter().map(|(_, inst)| inst).find(|inst| inst.op != Op::Nop);
                if next.is_some_and(|next| {
                    next.op == Op::UnsetLocal && next.immediate == Some(Immediate::LocalSlot(slot))
                }) {
                    self.moved_slot_loads.insert(result);
                }
            }
        }
    }
}

/// Every value a terminator reads, block arguments included, one entry per occurrence.
fn terminator_operands(terminator: &crate::ir::Terminator) -> Vec<ValueId> {
    use crate::ir::Terminator;
    match terminator {
        Terminator::Br { args, .. } => args.clone(),
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => std::iter::once(*cond)
            .chain(then_args.iter().copied())
            .chain(else_args.iter().copied())
            .collect(),
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => std::iter::once(*scrutinee)
            .chain(cases.iter().flat_map(|case| case.args.iter().copied()))
            .chain(default_args.iter().copied())
            .collect(),
        Terminator::Return { value } => value.iter().copied().collect(),
        Terminator::Throw { value } => vec![*value],
        Terminator::GeneratorSuspend {
            key,
            value,
            resume_args,
            ..
        } => key
            .iter()
            .chain(value.iter())
            .copied()
            .chain(resume_args.iter().copied())
            .collect(),
        Terminator::Fatal { .. } | Terminator::Unreachable => Vec::new(),
    }
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
    /// Where each `preg_match` pattern's compiled program was laid out, keyed by the pattern
    /// text: `(instruction base, class-bitmap base)`.
    ///
    /// Keyed by CONTENT for the same reason `default_strings` is — the lowering has the
    /// pattern's bytes, not a `DataId` for the program the planner built from them. Two call
    /// sites with the same pattern share one program.
    pub(super) regex_programs: &'a HashMap<String, (u32, u32)>,
    pub(super) static_slots: &'a super::statics::StaticSlots,
    /// The `(declaring class, property)` pairs some `unset($obj->$p)` in this module reaches.
    ///
    /// Built ONCE in `plan_module`, which is also what lays each pair's fatal message out, so the
    /// layout and the three lowering sites that read the marker cannot come to disagree about the
    /// set. Carried here rather than recomputed because the question is asked at every property
    /// READ and every `isset` probe, and answering it walks every instruction in the module — a
    /// per-read module walk is how this compiler's build times have run away before.
    pub(super) unset_properties: &'a [(String, String)],
    /// The suspended-state frame this function is a generator RESUME body for.
    ///
    /// `Some((layout, frame local))` only while lowering `<f>__resume`. A yield reads it
    /// to know where to spill, and the return terminator to know where the return value
    /// goes; every other function has `None` and behaves exactly as before.
    pub(super) generator: Option<(super::generators::GeneratorFrame, String)>,
    /// Dispatch segments, in emission order: `(block index, the instructions in it)`.
    ///
    /// One per block for an ordinary function. A generator's block splits after each
    /// `yield`, because a yield RETURNS and the resume has to re-enter past it — an EIR
    /// block index cannot name that point, since every yield this backend sees sits
    /// mid-block with code after it.
    pub(super) segments: Vec<(usize, Vec<InstId>)>,
    /// The segment being lowered, so a `yield` knows which arm to resume in.
    pub(super) segment: usize,
    /// The segment each block starts at, indexed by `BlockId::as_raw()`.
    pub(super) block_entry: Vec<u32>,
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
    /// Slots whose current ref-cell binding points at an owned kind-8 heap cell.
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
    /// Maps a reassigned by-value closure capture's slot raw id to an i32 local that is 1 once
    /// the slot holds a value of its own.
    ///
    /// The slot starts out BORROWING the callable descriptor's reference, and whether a store
    /// has replaced it is a runtime fact: a reassignment on one branch, or a second one, makes
    /// the static answer wrong in one direction or the other. The release before a store and
    /// the return epilogue both consult the flag, so the descriptor's value is never released
    /// and every value the body stored is released exactly once.
    pub(super) capture_owned_flags: HashMap<u32, String>,
    /// Whole-function use/definition facts, built on the first query. The ownership predicates
    /// that ask "is this value released / used elsewhere / produced by a copy" would otherwise
    /// rescan every instruction once per queried value.
    pub(super) usage_index: std::cell::OnceCell<UsageIndex>,
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
        // A dispatch arm is a SEGMENT, not a block. They are the same thing for every
        // function that does not yield — `plan_segments` produces one segment per block
        // there — and a generator's blocks split at each `yield`, so a branch to a block
        // has to name the segment the block STARTS at. One indirection, so the terminators
        // that set `$__state` did not each need to learn about it.
        self.block_entry
            .get(b.as_raw() as usize)
            .copied()
            .unwrap_or_else(|| b.as_raw())
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
    pub(super) fn iter_reserve(
        &mut self,
        iter: ValueId,
        elem: PhpType,
        is_hash: bool,
        dynamic: bool,
        dynamic_from_pointer: bool,
        is_generator: bool,
    ) {
        let n = self.temp_counter;
        self.temp_counter += 1;
        let source_local = self.fb.local(&format!("__iter_src{}", n), ValType::I32);
        let cursor_local = self.fb.local(&format!("__iter_cur{}", n), ValType::I64);
        let dynamic_kind = dynamic
            .then(|| self.fb.local(&format!("__iter_kind{}", n), ValType::I32));
        self.iter_state.insert(
            iter.as_raw(),
            IterSlots {
                source: source_local,
                cursor: cursor_local,
                elem,
                is_hash,
                dynamic_kind,
                dynamic_from_pointer,
                is_generator,
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
        // A BOXED source names no storage until the cell is read, so the container pointer, the
        // cursor seed and the kind all come from one runtime call — which also carries PHP's
        // warning for a value it will not iterate.
        if let Some(kind_local) = slots.dynamic_kind.clone() {
            let from_pointer = slots.dynamic_from_pointer;
            self.emit_load_value(source)?;
            // Same triple either way; only the question differs. A boxed value is asked its
            // cell TAG, an `iterable` pointer its heap KIND word.
            if from_pointer {
                self.fb.ins(
                    "call $__rt_iterable_iter_start",
                    "open a foreach over an iterable container pointer",
                );
            } else {
                self.fb.ins(
                    "call $__rt_mixed_iter_start",
                    "open a foreach over a boxed value",
                );
            }
            self.fb
                .ins(&format!("local.set {}", kind_local), "runtime container kind");
            self.fb
                .ins(&format!("local.set {}", cursor_local), "seeded cursor");
            self.fb
                .ins(&format!("local.set {}", source_local), "container pointer");
            return Ok(());
        }
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
    /// calls the dedicated kind-8 release helper with a typed payload tag and then
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

    /// The function's usage index, built on first use.
    pub(super) fn usage(&self) -> &UsageIndex {
        self.usage_index
            .get_or_init(|| UsageIndex::build(self.function))
    }

    /// Releases the objects, strings and containers held by static properties and static
    /// locals, after the globals.
    ///
    /// When the module has destructors they have already run (`__rt_run_all_destructors` ends
    /// the script's destructor phase and switches them off), so this only gives the memory
    /// back. Each slot is cleared before its release, so nothing is freed twice.
    pub(super) fn emit_static_shutdown_release(&mut self) {
        if super::statics::shutdown_release_slots(self.module, self.static_slots).is_empty() {
            return;
        }
        self.fb.ins(
            &format!("call ${}", super::statics::RT_RELEASE_STATICS_NAME),
            "release static properties and static locals",
        );
    }

    /// Deletes the `tmpfile()` handles nobody closed, at the very end of the teardown.
    pub(super) fn emit_tmpfile_teardown(&mut self) {
        if super::tmpfiles::module_has_tmpfile_teardown(self.module) {
            self.fb.ins(
                &format!("call ${}", super::tmpfiles::RT_TMPFILES_TEARDOWN_NAME),
                "php deletes a temporary file when its handle is freed at script end",
            );
        }
    }

    /// Flushes output buffers still open when `exit()` ends the script.
    pub(super) fn emit_exit_output_flush(&mut self) {
        if super::output_buffer::module_uses_output_buffering(self.module)
            && self.module.functions.iter().any(|function| function.flags.is_main)
        {
            self.fb.ins(
                "call $__rt_ob_flush_all",
                "php flushes output still buffered when exit() ends the script",
            );
        }
    }

    /// Emits `main`'s script-end teardown, in php's order.
    ///
    /// 1. Ref-cell owners and reassigned captures, as at any return.
    /// 2. The globals sweep: variables holding an object nothing else references are released
    ///    last-declared first, repeated until a pass frees nothing (`zend_call_destructors`).
    /// 3. Every other object still alive has its destructor run in handle order
    ///    (`zend_objects_store_call_destructors`), after which destructors are off.
    /// 4. What remains (locals, then statics) is NOT released: every destructor php would run
    ///    has run by now (or the module has none), destructors are switched off, and the
    ///    process ends right after the tmpfile teardown. Those releases only returned memory
    ///    to an allocator nobody will call again, and they
    ///    cost time on a program holding many objects. Nothing user-visible happens in them: a
    ///    stream write goes straight to `fd_write`, a generator frame release runs no `finally`
    ///    block, and a ref cell or capture release only frees. The output buffers are flushed
    ///    and the temp files deleted explicitly, above.
    ///
    /// Steps 2 and 3 exist only in a module with a destructor, where they are the observable
    /// part. `main` only runs in a command module, which ends in `proc_exit` right after this;
    /// a reactor module has no `main`, so the host can call it again and never reaches here.
    pub(super) fn emit_main_shutdown(&mut self, returned_slot: Option<LocalSlotId>) -> Result<()> {
        self.emit_ref_cell_owner_epilogue()?;
        self.emit_reassigned_capture_epilogue(returned_slot, None)?;
        if super::handles::module_has_destructors(self.module) {
            self.fb.ins(
                "call $__rt_object_drain_now",
                "objects parked by a deep release still owe their destructor (and a link word)",
            );
            self.fb.ins(
                &format!("(global.set ${} (i32.const 1))", super::function::SHUTTING_DOWN_GLOBAL),
                "from here an exception out of a destructor is final",
            );
            self.emit_global_destructor_sweep()?;
            self.fb.ins(
                "call $__rt_run_all_destructors",
                "run the remaining destructors in handle order",
            );
        }
        // php ends the output buffers after the destructors and BEFORE the executor frees the
        // globals and statics, so a handler still reads them.
        self.emit_exit_output_flush();
        // `returned_slot` only matters to the release this teardown no longer performs.
        let _ = returned_slot;
        self.emit_tmpfile_teardown();
        Ok(())
    }

    /// Runs `main`'s teardown and the process exit, for the landing pads that end the script
    /// away from a `return`: an `exit()` unwound from a callee (`status` is its local) and an
    /// exception nothing caught (`None`, status 255).
    ///
    /// A destructor that throws during the teardown is php's second fatal, so the whole
    /// sequence runs under the exception tag when the module has one.
    pub(super) fn emit_guarded_main_shutdown(&mut self, status: Option<&str>) -> Result<()> {
        let guarded = super::function::module_uses_exceptions(self.module);
        // An `exit()` inside a destructor the teardown runs unwinds with the exit tag, and the
        // landing that handles that tag has already closed by the time the teardown runs.
        let exit_guarded = super::function::module_unwinds_exit(self.module);
        if let (Some(local), true) = (status, exit_guarded) {
            // This landing can be reached FROM a teardown already under way (the normal script
            // end runs inside `main`'s exit landing, and so does the uncaught-exception pad): an
            // `exit()` out of one of its destructors. php stops there, running no destructor
            // after it, so the program ends with that status instead of tearing down again.
            self.fb.ins(
                &format!("(if (global.get ${}) (then", super::function::SHUTTING_DOWN_GLOBAL),
                "an exit() raised by a destructor of the teardown ends the script at once",
            );
            self.emit_exit_output_flush();
            self.fb.ins(&format!("local.get {local}"), "the status the destructor asked for");
            self.fb.ins("call $wasi_proc_exit", "WASI proc_exit(code)");
            self.fb.ins(
                "unreachable",
                "elephc-trap:post-noreturn:shutdown-exit-landing WASI proc_exit is non-returning",
            );
            self.fb.raw("))");
        }
        if exit_guarded {
            self.fb.raw("(block $__sd_exit (result i32)");
        }
        if guarded {
            self.fb.raw("(block $__sd_caught (result i32)");
        }
        if guarded || exit_guarded {
            let mut clauses = String::new();
            if guarded {
                clauses.push_str(&format!(
                    " (catch ${} $__sd_caught)",
                    super::function::EXCEPTION_TAG
                ));
            }
            if exit_guarded {
                clauses.push_str(&format!(" (catch ${} $__sd_exit)", super::function::EXIT_TAG));
            }
            self.fb.raw(&format!("(try_table{clauses}"));
        }
        self.emit_main_shutdown(None)?;
        match status {
            Some(local) => self.fb.ins(&format!("local.get {local}"), "the status exit() asked for"),
            None => self.fb.ins("i32.const 255", "PHP's fatal exit status"),
        }
        self.fb.ins("call $wasi_proc_exit", "WASI proc_exit(code)");
        self.fb.ins(
            "unreachable",
            "elephc-trap:post-noreturn:shutdown-proc-exit WASI proc_exit is non-returning",
        );
        if guarded || exit_guarded {
            self.fb.raw(")");
            self.fb.ins(
                "unreachable",
                "elephc-trap:proven-invariant:try-table-tail the guarded teardown ends in proc_exit",
            );
        }
        if guarded {
            self.fb.raw(")");
            self.fb.ins("drop", "the exception escaped a destructor");
            self.fb.ins(
                &format!(
                    "(call $__rt_fail (global.get ${}))",
                    super::function::EXCEPTION_FATAL_CODE_GLOBAL
                ),
                "php's fatal for it; no further destructor runs",
            );
            self.fb.ins(
                "unreachable",
                "elephc-trap:post-noreturn:shutdown-destructor-fatal runtime fatal helper does not return",
            );
        }
        if exit_guarded {
            self.fb.raw(")");
            let caught = self.fresh_temp(ValType::I32);
            self.fb.ins(
                &format!("local.set {caught}"),
                "exit() out of a teardown destructor: its status ends the program",
            );
            self.emit_exit_output_flush();
            self.fb.ins(&format!("local.get {caught}"), "the status the destructor asked for");
            self.fb.ins("call $wasi_proc_exit", "WASI proc_exit(code)");
            self.fb.ins(
                "unreachable",
                "elephc-trap:post-noreturn:shutdown-exit-caught WASI proc_exit is non-returning",
            );
        }
        Ok(())
    }

    /// The globals sweep of php's script end (`zend_call_destructors`).
    ///
    /// php walks the global symbol table from the LAST variable to the first and destroys each
    /// one holding an object whose refcount is exactly one; arrays, strings and shared objects
    /// are left for the later passes. It repeats while a pass destroys anything, because an
    /// object freed in one pass can leave another with a single owner. The symbol table order
    /// is the order variables first appear in the script, which is the order `main`'s local
    /// slots were declared in.
    fn emit_global_destructor_sweep(&mut self) -> Result<()> {
        let mut candidates: Vec<(u32, PhpType)> = Vec::new();
        for (index, local) in self.function.locals.iter().enumerate() {
            let raw = index as u32;
            if index < self.function.params.len()
                || !matches!(local.kind, LocalKind::PhpLocal)
                || local.name.is_none()
                || self.ref_cell_ptrs.contains_key(&raw)
                || !self.usage().stored_slots.contains(&raw)
            {
                continue;
            }
            let php_type = local.php_type.codegen_repr();
            if matches!(php_type, PhpType::Object(_) | PhpType::Mixed) {
                candidates.push((raw, php_type));
            }
        }
        if candidates.is_empty() {
            return Ok(());
        }
        candidates.reverse();
        let changed = self.fresh_temp(ValType::I32);
        let inner = self.fresh_temp(ValType::I32);
        self.fb.raw("(block $__gsweep_done (loop $__gsweep");
        self.fb
            .ins(&format!("(local.set {changed} (i32.const 0))"), "nothing freed yet this pass");
        for (raw, php_type) in candidates {
            let slot = LocalSlotId::from_raw(raw);
            let WasmRepr::Ptr(local) = self.slot_repr(slot)?.clone() else {
                continue;
            };
            if matches!(php_type, PhpType::Object(_)) {
                self.fb.ins(
                    &format!(
                        "(if (local.get {local}) (then (if (i32.eq (i32.load (i32.sub (local.get {local}) (i32.const 12))) (i32.const 1)) (then"
                    ),
                    "a global holding an object nobody else references",
                );
                self.emit_slot_release(slot, &php_type)?;
                self.fb
                    .ins(&format!("(local.set {changed} (i32.const 1))"), "this pass freed one");
                self.fb.raw("))))");
            } else {
                // A boxed variable: the cell and the object inside it must each have one owner.
                self.fb.ins(
                    &format!(
                        "(if (local.get {local}) (then (if (i32.eq (i32.load (i32.sub (local.get {local}) (i32.const 12))) (i32.const 1)) (then (if (i64.eq (i64.load (local.get {local})) (i64.const 6)) (then (local.set {inner} (i32.wrap_i64 (i64.load offset=8 (local.get {local})))) (if (i32.eq (i32.load (i32.sub (local.get {inner}) (i32.const 12))) (i32.const 1)) (then"
                    ),
                    "a boxed global holding an object nobody else references",
                );
                self.emit_slot_release(slot, &php_type)?;
                self.fb
                    .ins(&format!("(local.set {changed} (i32.const 1))"), "this pass freed one");
                self.fb.raw("))))))))");
            }
        }
        self.fb
            .ins(&format!("(br_if $__gsweep (local.get {changed}))"), "again while a pass freed something");
        self.fb.raw("))");
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
    ///
    /// `returned_string` is the pointer local of a returned string the slot walk cannot see
    /// (`return id($t);`): a string slot holding that very block is left alone, because the
    /// return hands it out and strings carry no refcount to say so.
    pub(super) fn emit_local_epilogue_cleanup(
        &mut self,
        returned_slot: Option<LocalSlotId>,
        returned_string: Option<&str>,
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
                let explicitly_stored = self.usage().stored_slots.contains(&raw);
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
            // A generator's wrapper took its own reference on every heap, string or callable
            // argument (`generators::emit_wrapper`), because its frame outlives the call; a body
            // that runs to its end gives those back here.
            let generator_owned = self.function.flags.is_generator
                && self
                    .function
                    .params
                    .get(index)
                    .is_some_and(super::generators::param_is_frame_owned);
            if matches!(php_type, PhpType::Array(_)) || generator_owned {
                candidates.push((raw, php_type));
            }
        }

        for (raw, php_type) in candidates {
            let slot = LocalSlotId::from_raw(raw);
            let guarded = self.emit_unless_returned_string(slot, returned_string)?;
            self.emit_slot_release(slot, &php_type)?;
            if guarded {
                self.fb.ins("end", "end of the release the return did not forbid");
            }
        }
        Ok(())
    }

    /// Opens an `if` that skips what follows when `slot` is a string slot holding the block
    /// `returned_string` points at, and answers whether it did (the caller closes it).
    fn emit_unless_returned_string(
        &mut self,
        slot: LocalSlotId,
        returned_string: Option<&str>,
    ) -> Result<bool> {
        let (Some(returned), WasmRepr::Str { ptr, .. }) = (returned_string, self.slot_repr(slot)?.clone())
        else {
            return Ok(false);
        };
        self.fb.ins(&format!("local.get {returned}"), "the returned string");
        self.fb.ins(&format!("local.get {ptr}"), "the slot's block");
        self.fb.ins("i32.ne", "the return does not hand this block out");
        self.fb.ins("if", "release only a block the return leaves behind");
        Ok(true)
    }

    /// Releases one local slot's owned heap value and CLEARS the slot.
    ///
    /// Clearing first is what makes a second pass — the return epilogue, or another explicit
    /// release — a no-op instead of a double free: every release path below is null-safe.
    /// Shared by the epilogue and by the explicit `ReleaseLocalSlot` an early scope exit emits.
    pub(super) fn emit_slot_release(
        &mut self,
        slot: LocalSlotId,
        php_type: &PhpType,
    ) -> Result<()> {
        let php_type = php_type.clone();
        {
            let repr = self.slot_repr(slot)?.clone();
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
    /// A string capture holding the block `returned_string` points at is left alone, as in
    /// `emit_local_epilogue_cleanup`.
    pub(super) fn emit_reassigned_capture_epilogue(
        &mut self,
        returned_slot: Option<LocalSlotId>,
        returned_string: Option<&str>,
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
            // On a path where no store ran, the slot still holds the descriptor's value.
            let flag = self.capture_owned_flags.get(&raw).cloned();
            if let Some(flag) = &flag {
                self.fb.ins(&format!("local.get {flag}"), "did this call store into the capture?");
                self.fb.ins("if", "release only a value this frame stored");
            }
            let guarded = self.emit_unless_returned_string(slot, returned_string)?;
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
            if guarded {
                self.fb.ins("end", "end of the release the return did not forbid");
            }
            if flag.is_some() {
                self.fb.ins("end", "end owned-capture release");
            }
        }
        Ok(())
    }
}
