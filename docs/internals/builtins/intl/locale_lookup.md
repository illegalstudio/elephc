---
title: "locale_lookup() - internals"
description: "Compiler internals for locale_lookup(): lowering path, type checks, and runtime helpers."
---

## `locale_lookup()` - internals

## Where it lives

- **Signature**: [`src/builtins/intl/locale_lookup.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/intl/locale_lookup.rs)
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
function locale_lookup(array $languageTag, string $locale, bool $canonicalize = false, ?string $defaultLocale = null): ?string
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `locale_lookup()`](../../../php/builtins/intl/locale_lookup.md)
