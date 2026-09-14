---
title: "xmlwriter_end_comment() - internals"
description: "Compiler internals for xmlwriter_end_comment(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 996
---

## `xmlwriter_end_comment()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_xml.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_xml.rs)
- **Lowering**: [`src/xml_prelude/build/writer.rs`:1018](https://github.com/illegalstudio/elephc/blob/main/src/xml_prelude/build/writer.rs#L1018) (`xmlwriter_end_comment`)
- **Function symbol**: `xmlwriter_end_comment()`


### Lowering notes

- Implemented by the compiler-injected xml prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function xmlwriter_end_comment(mixed $writer): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_comment.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_comment.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `xmlwriter_end_comment()`](../../../php/builtins/xml/xmlwriter_end_comment.md)
