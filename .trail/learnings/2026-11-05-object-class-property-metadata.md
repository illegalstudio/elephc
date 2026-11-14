---
title: Object properties come from a 16-byte layout table emitted as @class/@prop markers
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 150 }
touches:
  - src/codegen/block_emit.rs
  - src/debug_metadata.rs
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/lib.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/debug-metadata.md
  - docs/compiling/dap.md
---

## Decided

- Class metadata is derived from `Module.class_infos`, not a second AST walk.
  User classes emit one `@class` marker (id, hex name, parent, abstract, internal)
  followed by one `@prop` marker per layout slot (name, offset, PHP type,
  visibility, reference-cell flag, typed-property flag).
- Property offsets are `8 + index * 16`, matching the object layout: class id at
  object[0], then 16-byte slots. The low word carries the value/pointer and the
  high word carries a string length, a tagged-scalar tag or the uninitialized
  sentinel.
- The debugger's value reader (`crates/elephc-debug/src/value.rs`) holds the class
  table by class id. Objects render as `object(Point) (3)` and expose declared
  properties as children; nested arrays and objects recurse through the same
  read-only formatter.
- Property types use the same `typed_value` decoder as locals: mixed cells,
  arrays/hashes, strings, tagged scalars, ints/floats/bools, resources,
  pointers, references and uninitialized typed slots.
- Private and protected properties are named `label (private)` /
  `label (protected)` in the DAP/terminal child list. This uses the emitted
  visibility metadata.
- Internal and compiler-injected classes emit their name only, without property
  slots. Their object payloads can be compact/custom (Throwable, Fiber,
  Generator, SPL), so reading `8 + index*16` as ordinary properties is not safe.

## Tried and ruled out

- Reusing `_class_vd_desc_*`/`_class_prop_desc_*` runtime descriptors: they are
  assembly tables with target-specific encodings and are used by `var_dump` /
  `print_r`, which allocate and can run user code. The debugger needs a
  side-effect-free read, and the compiler already has the layout data.
- Emitting only public properties: private/protected slots are part of the
  object and are the most useful during debugging. Visibility is carried in the
  name instead.
- Emitting internal class properties: custom payloads make the regular property
  walk unsafe. The class name is still emitted, so `object(Exception)` displays
  correctly without children.

## Watch out

- Dynamic properties and `__debugInfo()` are not represented. They need the
  per-object side-table or a non-mutating descriptor path.
- Hidden private parent slots can share a name with a child property; the
  current child list would show duplicates. A declaring-class suffix is the next
  polish.
- Property hooks / virtual properties do not have a backing slot; they are not
  in `ClassInfo.properties` unless they have storage.
- Static properties are excluded from the object children; they are class state,
  not instance state.
- Class ids and marker offsets are compile-local. Never persist them across a
  rebuild.
- The DAP e2e test covers a promoted public/public/private `Point` object:
  `$point` = `object(Point) (3)`, `x int(2)`, `y int(3)`, `label (private)
  string(6) "origin"`.
