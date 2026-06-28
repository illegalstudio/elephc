---
title: "function_exists() — internals"
description: "Compiler internals for function_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 58
---

## `function_exists()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:758](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L758) (`lower_function_exists`)
- **Function symbol**: `lower_function_exists()`


### Lowering notes

- Lowers `function_exists("name")` for compile-time string names.
- Recognizes user functions, externs, catalog builtins, and the date/time procedural aliases
- that `name_resolver` desugars (including the injected timezone-introspection prelude
- functions). The aliases are matched through `is_date_procedural_alias` rather than the catalog
- because their call sites are rewritten before codegen, so they never reach the builtin catalog
- yet must still report as existing to match PHP.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function function_exists(string $function): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `function_exists()`](../../../php/builtins/class/function_exists.md)

