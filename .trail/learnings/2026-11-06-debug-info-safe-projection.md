---
title: Runtime-only __debugInfo() needs a safe static projection, not a call while stopped
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 150 }
touches:
  - src/debug_projection.rs
  - src/codegen/block_emit.rs
  - src/debug_metadata.rs
  - crates/elephc-debug/src/value.rs
  - crates/elephc-debug/src/dap.rs
  - crates/elephc-debug/src/lib.rs
  - tests/codegen/cli.rs
  - docs/compiling/debug-metadata.md
  - docs/compiling/dap.md
  - .plans/native-debugger.md
---

## Decided

- The debugger gets its own **safe structural projection** of `__debugInfo()`,
  separate from `var_dump`'s fold. It accepts a single `return [...]` whose values
  are:
  - direct `$this->property` reads of a declared property;
  - int/float/string/bool/null literals (including `-N` for numeric literals);
  - literal-only nested arrays with string/int keys.
- Literal arrays are rendered at compile time in the debugger's PHP-style form
  (`array(3) {[0] => int(1), …}`). Strings are escaped once; a string longer than
  256 bytes, an array deeper than 3 levels, an array wider than 16 children, or a
  computed expression falls back to `runtime`.
- The method declaration is resolved through `method_impl_classes`, so a child
  class inheriting `__debugInfo()` keeps the parent projection. Property reads
  are checked against the **object's** class layout, not the declaring parent.
- `.edbg.json` projection entries are either `{key, property}` for live reads or
  `{key, value, php_type}` for display-only literals. `@debuginfo` markers carry
  `property_hex` or `value_hex`/`type_hex`.
- `ObjectChild` gains `static_text`/`static_type`. DAP, terminal `locals`/`p` and
  call capture render the static text directly and never allocate a runtime value
  or execute the method. Static entries have `variablesReference: 0`.
- A computed or multi-statement body still emits `debug_info.kind = "runtime"`;
  the object shows `object(C) (N) [__debugInfo() runtime]` and its declared
  properties. The method is never called while stopped.
- The new projection runs only while emitting `--debug` metadata, so normal
  builds pay nothing.

## Tried and ruled out

- Calling `__debugInfo()` while stopped: it can echo, allocate, mutate state,
  throw or terminate the process. The fixture's runtime body echoes
  `COMPUTED_CALLED`; the DAP test asserts that text never appears.
- Reusing `var_dump_debug_info_projection`: that fold returns descriptor rows
  (key + property slot), which cannot represent literal scalars or nested arrays.
  It remains the var_dump fast path; the debugger has its own richer model.
- A general constant evaluator: arithmetic, calls, casts and branches can hide
  side effects or runtime state. Only the explicitly safe shapes are projected.
- Rendering static arrays as expandable DAP containers: their values were never
  materialized at run time. Pre-rendered text with `variablesReference: 0` is
  honest and cannot pretend to read storage that does not exist.
- Truncating an oversized projection value: a partial view silently changes what
  PHP would show, and would need extra overflow flags through the sidecar and the
  debugger. The whole class falls back to `runtime` instead.
- Projecting a NUL-mangled key: that is php-src's visibility spelling, which the
  debugger does not reproduce.

## Watch out

- A static literal child has no backing slot. `dataBreakpointInfo` therefore
  returns null for it; only property-backed keys remain watchable.
- A property entry inside an inherited projection reads the child layout. A
  private parent property shadowed by the child is still resolved by name; that
  existing layout caveat is unchanged.
- The projection is only as good as the single-return AST. A body with a local
  temporary, `if`, `match`, concat or call is marked runtime even if a human can
  see it is constant.
- The string cap counts UTF-8 bytes, not characters; a 256-byte boundary can
  exclude a multi-byte character and the whole string then falls back to runtime.
- The `.edbg.json` literal text is display-only. A future consumer must not parse
  it back into values; it exists so the debugger can remain read-only.
- The e2e fixture asserts `StaticInfo` has 5 projection entries (`id`, `name`,
  `version`, `flags`, `meta`), `ComputedInfo` stays runtime with only `id`, and
  the program output is `75` without `COMPUTED_CALLED`.
