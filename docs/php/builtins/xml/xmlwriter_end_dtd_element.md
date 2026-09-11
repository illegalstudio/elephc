---
title: "xmlwriter_end_dtd_element()"
description: "Ends the current DTD element declaration."
sidebar:
  order: 1000
---

## xmlwriter_end_dtd_element()

```php
function xmlwriter_end_dtd_element(mixed $writer): bool
```

Ends the current DTD element declaration.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_element.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_dtd_element` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_dtd_element.md).
