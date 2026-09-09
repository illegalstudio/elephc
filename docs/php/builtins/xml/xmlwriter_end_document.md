---
title: "xmlwriter_end_document()"
description: "Ends the document, closing every open node."
sidebar:
  order: 936
---

## xmlwriter_end_document()

```php
function xmlwriter_end_document(mixed $writer): bool
```

Ends the document, closing every open node.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_document.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_document.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_document` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_document.md).
