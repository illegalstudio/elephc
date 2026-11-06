---
title: Property hooks are metadata, not an invitation to run getters while stopped
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 120 }
touches:
  - src/codegen/block_emit.rs
  - src/debug_metadata.rs
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/dap.rs
  - crates/elephc-debug/src/lib.rs
  - tests/codegen/cli.rs
  - docs/compiling/dap.md
  - docs/compiling/debug-metadata.md
---

## Decided

- Hooks are class metadata: `classes[].property_hooks` maps a PHP-visible property
  name to `{get, set, get_by_ref, virtual}`. Properties with no hooks stay out of
  the map, so existing metadata is unchanged.
- `@prop` comments carry `hooks=none|get|set|get+set`, `get_by_ref=0|1` and
  `virtual=0|1`. The metadata build inserts an entry only when at least one accessor
  exists.
- The object reader splits hooked properties into two read-only cases:
  - **virtual** (`virtual=true`): there is no backing slot. The child value is a
    placeholder and the DAP/terminal view prints `virtual [get hook not executed]`,
    `virtual [get/set hooks not executed]` or `virtual [write-only; set hook]`.
  - **backed getter**: `offset` still addresses the raw backing value. The view
    prints that value with `[backing value; get hook not executed]` (or
    `get-by-ref`) so the user does not mistake it for the accessor's result.
- A backed **set-only** hook needs no caveat: reading the slot is exactly what PHP
  observes (no getter is involved), so it renders normally.
- `dataBreakpointInfo` refuses a virtual property with `dataId: null` and an
  explanation: watching it would either read meaningless storage or run the getter.
  Backed hooked properties keep a real slot and remain watchable.
- A statically folded `__debugInfo()` entry that maps to a virtual property is
  rendered with the same marker instead of reading its placeholder slot.
- The terminal `locals`/`p` and `monitor --calls=full` reuse the annotated children,
  so all inspection surfaces agree.

## Tried and ruled out

- Running the getter to show the "real" value: it can allocate, mutate state, throw,
  or terminate the program, exactly like the `__debugInfo()` decision. Stopped means
  stopped.
- Displaying the virtual slot as `int(0)`: it is uninitialized/unused storage, not a
  value, and would silently lie about the property.
- Per-property `hooks` JSON fields on every property: it bloats the common case and
  made all existing test literals noisy. A class-level map is smaller and matches the
  metadata's class-oriented shape.
- Watching a virtual property's placeholder slot: its value never changes through
  the hook path, so a watch would never fire.
- Treating a short `set => expr;` hook as virtual: the parser's lowering wraps a short
  setter in an assignment to the backing slot, so it is backed by definition. A
  virtual setter must use a block body; the fixture uses one to exercise the virtual
  path.

## Watch out

- `virtual` means "hooks and no backing slot anywhere in the ancestry", not "has a
  getter". A backed getter is still virtual=false.
- The backing value can differ from the value PHP code sees (the getter may double
  it). The caveat is the contract; do not drop it when adding new formatters.
- Address-reuse and same-class caveats still apply to backed-hook data breakpoints.
- A property with only a set hook is readable storage but not writable through the
  debugger; the metadata exposes `set` but the view still shows the raw value.
- `property_hooks` is omitted when empty, so clients must default missing to "no
  hooks" (the Rust `Class` uses `#[serde(default)]`).
- The e2e fixture uses `fahrenheit` (virtual get/set), `doubled` (backed get/set,
  backing 4, visible 8) and `writeOnly` (virtual set-only); it asserts the three
  markers, the `(4)` child count, the virtual data-breakpoint refusal, and output
  `50 8`.
