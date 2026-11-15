---
title: Dynamic data breakpoints walk the per-object hash by raw key bytes
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 80 }
touches:
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/dap.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
---

## Decided

- `dataBreakpointInfo` first resolves the name against declared properties and
  folded `__debugInfo()` keys. If none matches and the class has a
  `dynamic_offset`, it walks the object's dynamic hash for the raw requested key
  and returns `dynamic:<address>:<class-id>:<offset>:<hex-key>`.
- The key is hex-encoded raw bytes, not a display string. PHP dynamic property
  names are byte strings; hex keeps `:`, quotes, and non-UTF-8 bytes out of the
  dataId grammar while round-tripping exactly.
- `setDataBreakpoints` validates the class still declares that dynamic offset and
  that the key currently exists, then snapshots the rendered value. A watch stores
  the object address, class id, offset and key bytes.
- `value::object_dynamic_property_value` revalidates the object class id, reads the
  hash pointer at the dynamic offset, walks the standard 64-byte hash entries,
  compares key bytes exactly, and decodes the entry's boxed `Mixed`/tagged value.
  A missing key or freed/reallocated object returns `None` and the check is
  skipped.
- Each site transition re-walks the hash, so a rehash or a delete-and-recreate of
  the same key follows the object rather than a stale entry address.

## Tried and ruled out

- Reusing `object_property_value`: dynamic properties have no declared slot or
  offset; the hash entry is the storage.
- Encoding the key as a UTF-8 string in the dataId: property names can contain
  `:`, quotes and non-UTF-8 bytes. Hex is unambiguous and matches the raw bytes.
- Watching the dynamic hash pointer itself: a rehash changes the pointer without
  a semantic value change; the watch is on the key's value.
- Snapshotting the whole dynamic hash: an unrelated dynamic property write would
  trip the watch. The keyed lookup is exact.

## Watch out

- The same-class object-address-reuse caveat from declared property watches
  remains: a freed object whose address is reused by an object of the same class
  cannot be distinguished without a runtime lifetime registry.
- A dynamic property that is unset and later recreated with the same value does
  not stop; the watch compares values, not entry identity. A different recreated
  value does stop.
- Dynamic property names with characters that the DAP child list escapes (control
  bytes) are hex-encoded here; `dataBreakpointInfo` receives the displayed name,
  so the client should use a name that round-trips to the same raw bytes.
- The watch is checked once per source-line transition, so multiple writes between
  two sites report the last value.
- The e2e test stops at line 9 with `extra = "before"`, arms a dynamic watch,
  continues, stops at line 10 with `extra = "after"`, clears the watches, and
  finishes with output `7`.
