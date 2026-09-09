---
title: "xmlwriter_end_element()"
description: "Ends the current element, using the short form when it has no content."
sidebar:
  order: 966
---

## xmlwriter_end_element()

```php
function xmlwriter_end_element(mixed $writer): bool
```

Ends the current element, using the short form when it has no content.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_element.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_element` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_element.md).
