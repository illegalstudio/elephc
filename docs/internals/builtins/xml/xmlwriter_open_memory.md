---
title: "xmlwriter_open_memory() — internals"
description: "Compiler internals for xmlwriter_open_memory(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 945
---

## `xmlwriter_open_memory()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:971](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L971) (`xmlwriter_open_memory`)
- **Function symbol**: `xmlwriter_open_memory()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_open_memory(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_memory.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_memory.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_open_memory()`](../../../php/builtins/xml/xmlwriter_open_memory.md)
