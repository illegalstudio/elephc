---
title: "xmlwriter_end_attribute()"
description: "Ends the current attribute."
sidebar:
  order: 934
---

## xmlwriter_end_attribute()

```php
function xmlwriter_end_attribute(mixed $writer): bool
```

Ends the current attribute.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_attribute.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_attribute.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_attribute` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_attribute.md).
