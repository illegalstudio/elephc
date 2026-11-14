---
title: Object-property data breakpoints read the slot through a class-id revalidation
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 80 }
touches:
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- `dataBreakpointInfo` now accepts a `VariableContainer::Value` whose tag is an
  object. It resolves the class from the object's class id, matches the requested
  name against the declared properties (stripping the ` (private)` /
  ` (protected)` suffix used by the child list), and returns a
  `object:<address>:<class-id>:<offset>` dataId.
- `setDataBreakpoints` resolves the class and property from the metadata by
  class id/offset, validates that the object is still a managed object of that
  class, and snapshots the rendered property value. The watch stores the object
  address, class id, property offset and PHP type — not a frame address.
- Property watches are checked at every source-line transition, not only in the
  function that owned the object, because a setter or callee can mutate the
  object from another frame. Local watches remain scoped to their function.
- `value::object_property_value` first checks the uniform heap-kind byte and then
  the class-id word before reading the 16-byte slot. A freed/reused allocation
  whose kind or class no longer matches is skipped, not decoded. The same helper
  decodes the slot as `object_children` (ref-cells, tagged scalars, typed
  uninitialized slots, nested containers).
- A mismatch during `setDataBreakpoints` returns `verified: false` with
  "the watched object is no longer live" rather than arming a stale address.

## Tried and ruled out

- Watching only the local that holds the object: `dataBreakpointInfo` receives
  the object container, not its owning local, and a property can be reached
  through several locals. The address + class id identifies the slot directly.
- Re-resolving the object pointer from a local at each stop: the DAP client may
  inspect a temporary object with no local binding, and rebinding the local would
  silently retarget the watch. The address watch follows the object it was armed
  on.
- Refusing property watches until a lifetime registry exists: the class-id/heap
  kind revalidation covers the observable cases (array/string/integer reuse and
  different classes) and makes the common debugger workflow possible. The
  remaining same-kind/same-class reuse case is documented.

## Watch out

- If the object is freed and a new object of the same class is allocated at the
  same address, the watch reads the new object. A debug allocator generation or
  a weak-reference handshake would be needed to close this hole.
- The watch is not attached to dynamic properties, `__debugInfo()` projections or
  property hooks; only declared 16-byte slots are supported.
- The property name is matched by its plain name; if a parent and child class
  share a property name, the first layout slot is chosen. The dataId carries the
  offset, so `setDataBreakpoints` is unambiguous once the watch is created.
- The watch is checked once per source-line transition, so several writes between
  two sites report only the last value at the stop.
- The e2e test constructs `Counter(0)` at line 7, arms a `value` watch from the
  object's DAP container, continues, stops at line 8 with `$counter->value` equal
  to `int(1)`, clears the watches, and finishes with output `2`.
