---
title: Run-time definedness of possibly-undefined locals
author: Guillaume
date: 2026-11-16
branch: worktree-agent-aef22dfcb60536c4f
touches:
  - src/ir_lower/local_definedness.rs
  - src/ir/builder.rs
  - src/ir_lower/context.rs
  - src/ir_lower/stmt/exceptions.rs
  - src/name_resolver/expressions.rs
---

## Decided
- Possibly-undefined locals get a run-time I64 flag only when a read finds them possibly undefined (src/ir_lower/local_definedness.rs). The static "definitely defined" fact is a MARKER id kept in `initialized_slots` (synthetic ids from u32::MAX down for untracked names, the flag's own slot for tracked ones), so every branch merge intersects it for free.
- Two-pass lowering in numeric_storage::lower_body: pass 1 records names read without their marker, pass 2 (from a snapshot taken BEFORE begin_body) installs flags. The names persist across the outer string-storage replay so it installs them up front.
- The flag is set by Builder::emit_with_effects on every op naming the variable's slot (except loads, releases, operand pins, UnsetLocal, ZeroLocalSlot, and a StoreLocal of the slot's own reloaded value, which is a representation change). In-place writes ($a[] = .., by-ref fills) set it through note_slot_initialized / define_element_write_root.
- for/foreach keep their body's initialized slots after the loop (fine for storage, wrong for definedness): lower_stmt restores the markers around them. try: catches start from the entry markers, the join intersects the exits (exceptions.rs hooks).
- compact() is rewritten by the name resolver to `['a' => $a ?? null]` with ONE shared span on key/??/var/null; lowering recognizes that fingerprint (name_resolver::compact_entry_variable) and unsets the key with `compact(): Undefined variable $a` when the flag says undefined.
## Tried and ruled out
- Using "slot ∈ initialized_slots" as the definedness fact: loops and try leave slots initialized after a body that may not run, and the vivify seed initializes the slot at entry.
- Answering NULL for a plain read of a seeded `$a[] =` local: the checker types it `array` for the whole scope (seed_vivified_array_locals), so `return $out;` hit a TypeError at the inferred array return. Plain reads of seeded locals warn but keep the seed's []; probes and compact are exact.
- Guarding by-ref argument reads: byref($r) then got a copy and a false warning. Names a body may pass by reference (any method/closure/unknown call arg, resolved functions per param) are never tracked.
## Watch out
- `~/.cargo/bin/cargo` is a fork that ignores CARGO_TARGET_DIR; a cancelled background `cargo +1.98 build` can finish later and REPLACE target/debug/elephc, so the worktree binary is not a safe baseline for asm comparisons. Build the baseline in a separate `git worktree` (here .elephc-work/base).
- Codegen tests that need the eval bridge build `cargo build -p elephc-magician` from PATH; run with the magician staticlib prebuilt in the private target dir or they all fail with "failed to build bridge staticlib".
- Pre-existing, unfixed: `if ($c) { $o = new stdClass; $o->a = 1; } var_dump($o->a ?? 'x');` prints 'x' on the defined path (same on the baseline).
