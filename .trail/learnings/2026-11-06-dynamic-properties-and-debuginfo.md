---
title: Dynamic properties come from the object hash; __debugInfo() is a static projection, never a call
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 110 }
touches:
  - src/codegen/block_emit.rs
  - src/codegen_support/runtime/data/user.rs
  - src/codegen_support/runtime/data/mod.rs
  - src/debug_metadata.rs
  - crates/elephc-debug/src/value.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
  - docs/compiling/debug-metadata.md
---

## Decided

- A class layout reserves the dynamic-property hash at `8 + declared_property_count * 16`
  when `allow_dynamic_properties`, `eval_property_storage`,
  `clone_override_property_storage` or `scope_dynamic_property_storage` is set.
  `stdClass` is the one internal class whose payload is just a class id plus that
  hash at offset `8`. The class metadata carries this as `dynamic_offset`, so the
  debugger never guesses from the class name alone.
- Dynamic properties are written through `__rt_hash_set` with string keys and
  boxed `Mixed` values, so the debugger reuses the standard hash walker. Object
  keys are displayed as plain names (`extra`), not array keys (`["extra"]`); the
  hash walker gained a `HashKeyStyle` to keep the two callers separate.
- `__debugInfo()` is compiler-foldable only for the single-`return` associative
  literal of `$this->property` reads (`var_dump_debug_info_projection`). Those
  pairs are emitted as `@debuginfo` markers and stored in the class metadata as
  `{kind: "projection", entries: [{key, property}]}`. The debugger then displays
  the array key and reads the property slot, exactly matching `var_dump`.
- A `__debugInfo()` body that needs execution (arithmetic, calls, multiple
  statements) is marked `{kind: "runtime"}`. The debugger does **not** call it:
  calling user code while stopped can allocate, mutate state, throw, or exit via
  the `__debuginfo() must return an array` fatal. Such objects display
  `object(C) (N) [__debugInfo() runtime]` and list their declared/dynamic
  properties instead.
- The object child count includes dynamic hash length, so `object(Bag) (2)` is
  correct for one declared + one dynamic property without materializing values.

## Tried and ruled out

- Invoking `__rt_obj_debuginfo_fetch` from the debugger: it calls the PHP method
  with a guard, allocates the returned array, mutates `_print_r_*` and
  `_debuginfo_guard`, and can terminate the process on a bad return. That breaks
  read-only inspection and the stopped-state guarantee.
- Reading only `ClassInfo.properties` and treating dynamic properties as missing:
  the hash is already in the object and uses the same layout as associative
  arrays; walking it costs one pointer load.
- Emitting the dynamic hash for every internal class: many internal payloads are
  compact/custom, so `8 + N*16` is not a property slot. Only `stdClass` gets the
  offset among internal classes.
- Naming dynamic properties with the array wrapper `["key"]`: DAP object
  properties are plain names; the wrapper would be confusing and would break
  `dataBreakpointInfo` name matching later.

## Watch out

- Dynamic-property data breakpoints are not wired yet: `dataBreakpointInfo` only
  searches declared properties, so a dynamic property returns `dataId: null`.
- A dynamic hash key can duplicate a parent-private property's plain name; the
  declared child list uses `(private)` suffixes, but the dynamic entry is plain.
- `stdClass` is covered by the layout rule, but `__debugInfo()` on a user
  subclass of an internal class follows the same projection/runtime rules.
- The folded projection only handles `$this-><declared property>` values. A
  projection that reads a dynamic property, calls a method or uses a computed
  key stays `runtime`.
- The e2e test uses `Bag` (one declared + one dynamic), `Projected`
  (`['identifier' => $this->x]`) and `Computed` (`$this->x + 1`): it asserts
  `object(Bag) (2)` with `id int(7)`/`extra string(3) "dyn"`,
  `object(Projected) (1)` with `identifier int(4)`, and
  `object(Computed) (1) [__debugInfo() runtime]` with `x int(5)`.
