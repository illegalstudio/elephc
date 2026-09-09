---
title: "xmlwriter_end_dtd()"
description: "Ends the current DTD."
sidebar:
  order: 962
---

## xmlwriter_end_dtd()

```php
function xmlwriter_end_dtd(mixed $writer): bool
```

Ends the current DTD.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_dtd` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_dtd.md).
