---
title: "xmlwriter_write_pi()"
description: "Writes a complete processing instruction."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_pi` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_pi.md).
