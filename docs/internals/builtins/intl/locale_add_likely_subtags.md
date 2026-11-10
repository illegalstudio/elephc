---
title: "locale_add_likely_subtags() - internals"
description: "Compiler internals for locale_add_likely_subtags(): lowering path, type checks, and runtime helpers."
---

## `locale_add_likely_subtags()` - internals

## Where it lives

- **Signature**: [`src/builtins/intl/locale_add_likely_subtags.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/intl/locale_add_likely_subtags.rs)
- **Lowering**: [`src/builtins/semantics.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `eir_graph` strategy from the single-source builtin descriptor.
- Emits backend-neutral EIR primitives or a small EIR graph through `BuiltinLoweringContext`.

## Semantic descriptor

- **Target strategy**: `eir_graph`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `fresh`
- **Effects**: `static (20 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: descriptor-emitted EIR primitives or graph; no opaque builtin call remains.

## Signature summary

```php
function locale_add_likely_subtags(string $locale): string|false
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `locale_add_likely_subtags()`](../../../php/builtins/intl/locale_add_likely_subtags.md)
