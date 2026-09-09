---
title: "xmlwriter_output_memory() — internals"
description: "Compiler internals for xmlwriter_output_memory(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 947
---

## `xmlwriter_output_memory()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1435](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1435) (`xmlwriter_output_memory`)
- **Function symbol**: `xmlwriter_output_memory()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_output_memory(mixed $writer, bool $flush = true): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_output_memory.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_output_memory.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_output_memory()`](../../../php/builtins/xml/xmlwriter_output_memory.md)
