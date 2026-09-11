---
title: "xmlwriter_full_end_element()"
description: "Ends the current element with an explicit end tag."
sidebar:
  order: 945
---

## xmlwriter_full_end_element()

```php
function xmlwriter_full_end_element(mixed $writer): bool
```

Ends the current element with an explicit end tag.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_full_end_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_full_end_element.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_full_end_element` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_full_end_element.md).
