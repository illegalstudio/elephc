---
title: "xmlwriter_write_pi()"
description: "Writes a complete processing instruction."
sidebar:
  order: 1034
---

## xmlwriter_write_pi()

```php
function xmlwriter_write_pi(mixed $writer, string $target, string $content): bool
```

Writes a complete processing instruction.

**Parameters**:
- `$writer` (`mixed`)
- `$target` (`string`)
- `$content` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_pi.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_pi.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_pi` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_pi.md).
