# Hash element reference aliases (`$b =& $h['key']`)

> **Follow-up of PR #445 (issue #331):** indexed-array element references
> (`$b =& $a[0]`) shipped with two documented limits — assoc arrays (hashes)
> are rejected at check time, and the indexed bind neither pins COW storage
> nor autovivifies. This plan closes the hash side and aligns the indexed
> bind with the existing `ArrayElemAddr` machinery.

## Context

PR #445 added `Op::LoadArrayElemRefCell`: `$b =& $a[0]` computes the address
of an indexed-array element's **inline storage** and binds `$b` to it
non-owning via `BindRefCellPtr` (`src/ir_lower/context.rs:968`,
`bind_local_ref_cell_ptr`). The checker arm
(`src/types/checker/stmt_check/assignments/locals.rs`, `ExprKind::ArrayAccess`)
rejects everything that is not `PhpType::Array(_)` + `Int` index with
"Reference assignment to an array element requires an indexed array".

The inline-pointer contract cannot be reused for hashes:

1. **Entry layout.** Hash entries are 64-byte slots
   (`docs/internals/memory-model.md`): `value_lo` at +24, `value_hi` at +32,
   `value_tag` at +40. The per-entry `value_tag` is authoritative for reads,
   iteration, JSON, search, and GC. A raw pointer to `value_lo` cannot honor
   stores that change the tag, and `{lo, hi, tag}` does not match the 16-byte
   `{payload, tag}` layout that `TaggedScalar` ref cells assume.
2. **Rehashing.** Insertion can grow and rehash the table
   ("Rehashing during growth and copy-on-write cloning"), moving every entry.
   An inline pointer taken before an insert dangles after it.
3. **PHP semantics.** `$b =& $h['x']` on a missing key **creates** the entry
   with `null` (autovivification), and `unset($h['x'])` afterwards must leave
   `$b` holding the value (the reference outlives the table link).

Separately, the by-ref **call-argument** path already solved the indexed-array
half of this problem: `Op::ArrayElemAddr`
(`src/codegen/lower_inst/arrays.rs`, `lower_array_elem_addr`) clamps negative
offsets, calls `__rt_array_ensure_unique`, grows + dense-fills up to the
requested slot, and writes the possibly-reallocated array pointer back to the
source local/global. The `=&` bind added in PR #445 does **none** of that —
so `$c = $a; $b =& $a[0]; $b = 9;` currently mutates `$c` through shared COW
storage (PHP keeps `$c` unchanged), and OOB binds alias a null cell instead
of autovivifying.

## Goal

- `$b =& $h['key']` and `$b =& $h[$k]` (string or int keys) work on
  heterogeneous (`Mixed`-valued) assoc arrays with PHP semantics:
  autovivification of missing keys as `null`, COW pinning at bind, writes
  visible in both directions, `unset($h['key'])` leaves the alias holding
  the value.
- The indexed `=&` bind adopts the `ArrayElemAddr` semantics (ensure-unique,
  grow/dense-fill, source writeback) instead of the current
  no-pin/null-cell behavior.
- Every supported target (`macos-aarch64`, `linux-aarch64`, `linux-x86_64`)
  in the same change; EIR-only (no legacy backend work).

## Non-goals

- Homogeneous typed hashes (`AssocArray<Str, Int>` etc.) as ref sources —
  phase 2 (see Design §5). The checker keeps a clear diagnostic.
- `foreach ($h as $k => &$v)` by-ref iteration (separate feature; the boxed
  design below is forward-compatible with it).
- References into nested containers (`$b =& $h['a']['b']`), `Mixed`-typed
  receivers, and ref-into-hash for by-ref **call arguments** (a
  `HashElemAddr` sibling of `ArrayElemAddr`) — future work items.
- Converting indexed arrays to hashes on sparse binds. Indexed stays dense:
  `$a = [1,2,3]; $b =& $a[10];` dense-fills 3..10 (count 11), diverging from
  PHP's sparse key set (count 4). Same divergence the by-ref call-arg path
  already accepted; documented in `docs/php/types.md`.

## Alternatives considered

- **Inline pointer to `value_lo` (the indexed approach).** Rejected: dangles
  on rehash, cannot update `value_tag` on stores, layout mismatch with the
  `TaggedScalar` cell contract (tag at +16, not +8).
- **New 3-word "tagged entry cell" ref-cell flavor.** Workable but still
  dangles on rehash, and every ref-bound load/store site in
  `src/codegen/lower_inst.rs` (BindRefCellPtr consumers) would grow a third
  layout. Rejected in favor of reusing the existing boxed `Mixed` contract.
- **Boxed `Mixed` cell promotion (chosen).** The heap already has a boxed
  Mixed kind (heap kind 5) with refcounting, `__rt_mixed_unbox`, deep-free,
  and GC integration ("Alias safety comes from the Mixed box refcount",
  `docs/internals/memory-model.md`). Hash entries already store boxed cells
  for heterogeneous values (entry `value_tag` 7). Promoting the aliased
  entry's value to a box and sharing the box is PHP's zval-ref model:
  rehash-stable (the entry stores a pointer; the box never moves), tag
  changes live inside the box, and the box refcount gives `unset()` /
  lifetime semantics for free.

## Design

### 1. Bind semantics (runtime)

New runtime helper `__rt_hash_elem_ref_box` (leaf file
`src/codegen_support/runtime/arrays/hash_elem_ref_box.rs`, one emitter):

- **Inputs** (AArch64 / x86_64): `x0`/`rdi` = hash ptr (already
  ensure-unique'd by the caller), `x1`/`rsi` = key ptr or int-key payload,
  `x2`/`rdx` = key len, `-1` sentinel for integer keys (same convention as
  the entry layout).
- Normalizes the key like every other hash path (numeric strings → int keys).
- Probes for the key. **Missing** → insert a new entry whose value is a
  freshly allocated boxed Mixed cell holding `null`, entry `value_tag` = 7.
  Insertion may grow/rehash (reuses the existing insert/grow helpers; fatal
  "array capacity exceeded" propagates unchanged).
- **Present with `value_tag` 7** → reuse the existing box.
- **Present with a concrete scalar/container tag** → allocate a box, move the
  entry's current value into it (retaining refcounted payloads, no copy),
  rewrite the entry to `value_tag` 7 + box pointer.
- `__rt_incref` the box (the alias holds one strong reference) and return it
  (`x0`/`rax`).

The caller (EIR lowering, §3) is responsible for `__rt_hash_ensure_unique`
**before** the call and for writing the possibly-cloned hash pointer back to
the source slot — same store-back/writeback pattern as
`lower_array_elem_addr` (`ctx.store_value_to_local` +
`ctx.writeback_global_array_source`).

### 2. New EIR op

`Op::LoadHashElemRefCell` in `src/ir/instr.rs`:

- Operands: `[hash_value, key_value]` (key is `Str` or `Int`).
- Result: the box pointer, result type `PhpType::Mixed`.
- Effects: `READS_HEAP | WRITES_HEAP | MAY_FATAL` (ensure-unique clone,
  insert/grow, allocation can fatal). Mirror the `ArrayElemAddr |
  ArraySetMixedKey` arm at `src/ir/instr.rs:472`.
- Textual name `load_hash_elem_ref_cell`; validator arm next to
  `LoadArrayElemRefCell` (`src/ir/validator.rs:419`): two operands, result
  required.

### 3. EIR lowering

`lower_ref_assign_hash_elem` in `src/ir_lower/expr/mod.rs`, dispatched from
the same `ExprKind::ArrayAccess` ref-assign entry point that today calls
`lower_ref_assign_array_elem` (choose by the receiver's checked type):

1. Lower the hash and key expressions (source order).
2. Emit ensure-unique + store-back exactly like the `ArrayElemAddr` lowering
   does for indexed arrays (the prepare/writeback halves live in codegen —
   see §4 — the lowering only needs to pass the source-slot info the same
   way `Op::ArrayElemAddr` receives it today).
3. `ctx.emit_value(Op::LoadHashElemRefCell, vec![hash, key], …,
   PhpType::Mixed, …)`.
4. `ctx.bind_local_ref_cell_ptr(target, box_ptr, PhpType::Mixed, span)` —
   but with **owning** alias semantics (§5): the alias holds a box refcount,
   so rebinding/scope-exit must `__rt_decref` the box rather than skip
   release. Extend `bind_local_ref_cell_ptr` (or add a sibling
   `bind_local_owned_box_ref`) so the ownership walker sees the local as
   owning one reference to a kind-5 heap block.

### 4. Backend codegen

`src/codegen/lower_inst/arrays.rs` (or a new focused leaf
`src/codegen/lower_inst/hash_ref.rs` if the shared file crowds):
`lower_load_hash_elem_ref_cell` for both arches:

- ensure-unique via `__rt_hash_ensure_unique`, store-back + global writeback
  (copy the `lower_array_elem_addr` skeleton: `source_load_local_slot`,
  `ctx.store_value_to_local`, `ctx.writeback_global_array_source`).
- Materialize key regs: string keys as ptr/len, integer keys as payload +
  `-1` len sentinel (match the existing hash-get key materialization).
- `bl __rt_hash_elem_ref_box` / `call __rt_hash_elem_ref_box`, store the
  integer result into the instruction result value.
- Every `emitter.instruction` line carries the column-81 `//` comment.

### 5. Alias local semantics (`Mixed` ref-bound local)

The alias `$b` is a `Mixed` local whose value **is** the shared box pointer.
That makes reads work unchanged (Mixed locals already hold box pointers and
every consumer unboxes via `__rt_mixed_unbox`). The delta is on **stores**:

- Plain `Mixed` assignment allocates/replaces the box pointer in the slot —
  that would break the alias. Ref-bound Mixed locals must instead mutate the
  box **in place**: deep-free the previous payload
  (`__rt_mixed_free_deep`-style release of the old contents, not the box),
  then write the new tag/payload into the same cell. Add runtime helper
  `__rt_mixed_store_in_place` (leaf
  `src/codegen_support/runtime/arrays/mixed_store_in_place.rs`) with inputs
  box ptr + tag + lo/hi, and route stores to ref-bound Mixed locals through
  it in the `StoreLocal` lowering for ref-bound slots
  (`src/codegen/lower_inst.rs`, BindRefCellPtr consumers).
- This is also the reason phase 1 restricts sources to `Mixed`-valued hashes:
  every existing read path already tag-dispatches on entry `value_tag` 7.
  Homogeneous typed hashes (`AssocArray<Str, Int>`) read `value_lo` assuming
  the declared type; teaching those fast paths a tag-7 deref branch is
  phase 2 and out of scope here.

### 6. Checker

`src/types/checker/stmt_check/assignments/locals.rs`, `ExprKind::ArrayAccess`
arm:

- Accept `PhpType::AssocArray { key, value }` when `value` is `Mixed` and
  the index type is `Str` or `Int`. Target type: `Mixed`. Mark the target in
  `active_ref_params` and the **source hash name reference-exposed** (the
  propagation ledger treats reference-exposed names as volatile —
  `src/optimize/` counterpart in §7).
- `AssocArray` with a non-`Mixed` value type → new diagnostic:
  "Reference assignment to an assoc-array element requires a mixed-valued
  array (heterogeneous values); homogeneous value types are not supported
  yet". Keep the existing indexed/int-index errors for `Array(_)`.

### 7. Optimizer / effects

`$t =& $h['k']` **writes** the hash (ensure-unique clone, possible insert)
and exposes both names to aliasing:

- The ref-assign statement's effect modeling in `src/optimize/effects/` must
  report a write to the source hash local (today the ArrayAccess ref-assign
  source is only read). Reference-exposed names already go volatile in
  propagation (see `.plans/memory-model-aware-propagation.md`); extend that
  marking to the ArrayAccess-source case so no stale array fact survives the
  bind.
- No new EIR pass work: `LoadHashElemRefCell` is `WRITES_HEAP | MAY_FATAL`,
  which keeps DSE/CSE/LICM away by the existing eligibility rules.

### 8. Indexed `=&` alignment (COW + autovivification)

Rework `lower_load_array_elem_ref_cell`
(`src/codegen/lower_inst/arrays.rs`) to reuse the `ArrayElemAddr`
prepare/writeback halves instead of the raw address computation:

- `__rt_array_ensure_unique` + store-back + global writeback (fixes the COW
  violation: `$c = $a; $b =& $a[0]; $b = 9;` must leave `$c` unchanged).
- Grow + dense-fill for `index >= len` (replaces the null-cell fallback),
  clamp negative indices to slot 0 exactly like
  `lower_array_elem_addr_prepare_*` — keeping the two by-ref surfaces
  behaviorally identical.
- The EIR lowering (`lower_ref_assign_array_elem`) passes the source-slot
  info the same way the `ArrayElemAddr` path does today.

The inline-pointer contract stays for indexed arrays (typed dense storage is
the perf model); the documented "array must remain live / not grow while the
alias is used" caveat also stays and is called out in docs.

## PHP oracles (cross-check with `php -r` before locking test expectations)

```bash
php -r '$h=["a"=>1]; $b =& $h["x"]; var_dump($h, $b);'          # x created as NULL
php -r '$h=["a"=>1]; $b =& $h["a"]; $b=9; var_dump($h["a"]);'    # int(9)
php -r '$h=["a"=>1]; $b =& $h["a"]; $h["a"]="s"; var_dump($b);'  # string(1) "s"
php -r '$h=["a"=>1]; $c=$h; $b =& $h["a"]; $b=9; var_dump($c["a"]);' # int(1) — COW pin
php -r '$h=["a"=>1]; $b =& $h["a"]; unset($h["a"]); var_dump($b, $h);' # b keeps 1, h empty
php -r '$a=[1,2,3]; $c=$a; $b =& $a[0]; $b=9; var_dump($c[0]);'  # int(1) — indexed COW pin
php -r '$a=[1,2,3]; $b =& $a[5]; var_dump(count($a));'           # PHP: 4 (sparse) — elephc: 7 (dense, documented divergence)
```

---

# Implementation Plan

> Execute task-by-task; each task ends with focused tests passing and a
> commit. Build must stay warning-free; every new Rust file needs the module
> preamble and per-function docblocks; every `emitter.instruction` needs the
> column-81 comment.

## Global constraints (from AGENTS.md)

- All three supported targets in the same change; no ARM64-first stubs.
- EIR path only (`src/ir_lower/`, `src/codegen/lower_inst/`); never the
  legacy backend.
- One emitter function per runtime leaf file; data symbols in
  `src/codegen_support/runtime/data/` if any (none expected here).
- Focused tests locally (`cargo test --test codegen_tests <filter>`); CI
  runs the full matrix.
- PHP equivalence cross-checked with `php -r` for every new test expectation.

### Task 1: Runtime helper `__rt_hash_elem_ref_box`

**Files:**
- Create: `src/codegen_support/runtime/arrays/hash_elem_ref_box.rs`
- Modify: `src/codegen_support/runtime/arrays/mod.rs` (re-export),
  `src/codegen_support/runtime/emitters.rs` (emission call)

**Steps:**
- [ ] Implement the emitter per Design §1 for AArch64 and x86_64 (reuse the
  existing hash probe/insert/grow helper routines rather than re-emitting
  probing logic; follow `hash_insert_owned.rs` for calling conventions).
  Mind the x86_64 saved-register rule: `[rbp-8/-16/-24]` alias pushed
  r12/r13/r14 in runtime emitters — pick non-aliasing scratch slots.
- [ ] Unit-test the rendered assembly shape like
  `print_r_buffer.rs`'s `#[cfg(test)]` module does (labels present for both
  targets, both arch variants emitted).
- [ ] `cargo test -p elephc --lib hash_elem_ref_box` → PASS; commit
  `feat(runtime): __rt_hash_elem_ref_box promotes hash entries to shared boxed cells`.

### Task 2: `Op::LoadHashElemRefCell` (IR + validator)

**Files:**
- Modify: `src/ir/instr.rs` (variant, effects arm, textual name),
  `src/ir/validator.rs` (operand/result arm next to `LoadArrayElemRefCell`)

**Steps:**
- [ ] Add the op per Design §2.
- [ ] Extend the validator test module with an accept case (2 operands +
  result) and a reject case (missing result), mirroring the
  `LoadArrayElemRefCell` tests.
- [ ] `cargo test -p elephc --lib validator` → PASS; commit
  `feat(ir): LoadHashElemRefCell op`.

### Task 3: In-place Mixed store for ref-bound locals

**Files:**
- Create: `src/codegen_support/runtime/arrays/mixed_store_in_place.rs`
- Modify: `src/codegen_support/runtime/arrays/mod.rs`,
  `src/codegen_support/runtime/emitters.rs`,
  `src/codegen/lower_inst.rs` (StoreLocal-to-ref-bound-Mixed routing)

**Steps:**
- [ ] Implement `__rt_mixed_store_in_place` per Design §5 (release previous
  payload deep, write new tag/lo/hi into the same box; box refcount
  untouched).
- [ ] Route stores to ref-bound `Mixed` locals through it. First locate the
  BindRefCellPtr store consumers in `src/codegen/lower_inst.rs` and confirm
  how scalar ref-bound stores dispatch today; extend, don't fork.
- [ ] Assembly-shape unit test as in Task 1; commit
  `feat(codegen): in-place Mixed stores through ref-bound locals`.

### Task 4: Checker acceptance + diagnostics

**Files:**
- Modify: `src/types/checker/stmt_check/assignments/locals.rs`
- Test: `tests/error_tests/` (references/assignments area)

**Steps:**
- [ ] Accept `AssocArray { value: Mixed, .. }` with `Str`/`Int` index; type
  the target `Mixed`; mark target + source per Design §6.
- [ ] Error tests: homogeneous-value hash → the new diagnostic;
  non-`Str`/`Int` key type → existing/index diagnostic; indexed-array arm
  unchanged.
- [ ] `cargo test --test error_tests references` → PASS; commit
  `feat(types): accept mixed-valued hash sources in reference assignment`.

### Task 5: EIR lowering + backend codegen for the hash bind

**Files:**
- Modify: `src/ir_lower/expr/mod.rs` (`lower_ref_assign_hash_elem`,
  dispatch from the ArrayAccess ref-assign entry),
  `src/ir_lower/context.rs` (owned-box ref bind, Design §3.4),
  `src/ir_lower/ownership.rs` (alias owns one box reference; balanced
  release on rebind/return/throw),
  `src/codegen/lower_inst.rs` (dispatch arm),
  `src/codegen/lower_inst/arrays.rs` or new `hash_ref.rs`
  (`lower_load_hash_elem_ref_cell`, Design §4)

**Steps:**
- [ ] Wire lowering → op → codegen per Design §3–4 (ensure-unique +
  store-back + writeback first, then the helper call).
- [ ] Codegen tests in `tests/codegen/references.rs`, expectations locked
  against the PHP oracles above: write-through (both directions),
  autovivified NULL read, COW pin, string + int keys, alias in a function
  body (stack-frame slots), global hash source (writeback path).
- [ ] `cargo test --test codegen_tests references` → PASS; commit
  `feat: hash element reference aliases ($b =& $h['key'])`.

### Task 6: Ownership/GC coverage

**Files:**
- Test: `tests/codegen/runtime_gc/` (references or a new `hash_ref.rs` module)

**Steps:**
- [ ] Tests: `unset($h['k'])` with live alias (alias keeps value; entry
  gone); alias outliving the hash (scope exit releases in either order, no
  double-free, heap-debug clean); rebinding the alias releases the previous
  box; cycle: hash entry box holding the hash itself is collected.
- [ ] Run with the heap-debug assertions enabled (the runtime_gc harness
  default); `cargo test --test codegen_tests runtime_gc` filtered to the new
  module → PASS; commit `test(gc): hash ref alias ownership coverage`.

### Task 7: Indexed `=&` alignment (COW + grow)

**Files:**
- Modify: `src/codegen/lower_inst/arrays.rs`
  (`lower_load_array_elem_ref_cell` reuses the `ArrayElemAddr`
  prepare/writeback halves), `src/ir_lower/expr/mod.rs`
  (`lower_ref_assign_array_elem` passes source-slot info)
- Test: `tests/codegen/references.rs`

**Steps:**
- [ ] Rework per Design §8. Delete the now-dead null-cell fallback labels.
- [ ] Regression tests from the oracles: indexed COW pin, `$b =& $a[5]`
  dense-fill (assert elephc's documented divergent count **and** the
  write-through), negative index clamped like `ArrayElemAddr`.
- [ ] `cargo test --test codegen_tests references` → PASS; commit
  `fix: pin COW storage and autovivify on indexed element ref binds`.

### Task 8: Effects/propagation hazard coverage

**Files:**
- Modify: `src/optimize/effects/` (ref-assign ArrayAccess source = write +
  reference exposure)
- Test: `tests/codegen/optimizer/`

**Steps:**
- [ ] End-to-end hazard test with a runtime-unknown guard (`$argc`) so the
  construct survives AST folding: constant-propagated hash fact must not
  survive `$b =& $h['k']; $b = <new>; echo $h['k'];`.
- [ ] `cargo test --test codegen_tests optimizer` filtered → PASS; commit
  `fix(optimize): reference exposure for hash element ref binds`.

### Task 9: Docs, example, ROADMAP

**Files:**
- Modify: `docs/php/arrays.md` (reference-aliases section: hash support,
  indexed grow semantics), `docs/php/types.md` (incompatibilities: dense
  fill on sparse indexed binds; homogeneous-hash restriction),
  `docs/internals/memory-model.md` (aliased entries are promoted to boxed
  cells, entry `value_tag` 7), `ROADMAP.md` (item under the next minor)
- Create/extend: `examples/references/main.php` (add a hash-alias block —
  the example exists from PR #445)

**Steps:**
- [ ] Update docs per the file list; keep Astro frontmatter intact; remove
  the "not supported" note for hash refs.
- [ ] Extend the example; `cargo run -- examples/references/main.php` and
  run the binary to confirm output.
- [ ] Commit `docs: hash element reference aliases`.

## Plan self-review

- Every task compiles and tests on its own; Tasks 1–3 are pure additions,
  Task 5 is the first behavior switch, Task 7 changes existing behavior and
  carries its own regressions.
- Open verification points intentionally embedded in tasks: the exact
  BindRefCellPtr store-dispatch shape (Task 3) and the ownership-walker hook
  for an owning alias (Task 5) must be confirmed against the code before
  implementation — both files are named.
- Phase 2 (homogeneous typed hashes: tag-7 deref branch in the typed hash
  get/set fast paths) and `HashElemAddr` for by-ref call arguments are
  deliberately out and listed in Non-goals.
