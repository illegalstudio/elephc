---
title: "xmlwriter_end_pi()"
description: "Ends the current processing instruction."
sidebar:
  order: 1003
---

## xmlwriter_end_pi()

```php
function xmlwriter_end_pi(mixed $writer): bool
```

Ends the current processing instruction.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_pi.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_pi.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_pi` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_pi.md).
