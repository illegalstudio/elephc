---
title: "xmlwriter_end_dtd_attlist()"
description: "Ends the current DTD attribute list declaration."
sidebar:
  order: 963
---

## xmlwriter_end_dtd_attlist()

```php
function xmlwriter_end_dtd_attlist(mixed $writer): bool
```

Ends the current DTD attribute list declaration.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_attlist.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_attlist.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_dtd_attlist` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_dtd_attlist.md).
