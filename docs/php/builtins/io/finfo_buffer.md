---
title: "finfo_buffer()"
description: "Identifies the type and encoding of binary string contents."
---

## finfo_buffer()

```php
function finfo_buffer(finfo $finfo, string $string, int $flags = 0, ?resource $context = null): string|false
```

Identifies the type and encoding of binary string contents.

**Parameters**:
- `$finfo` (`finfo`)
- `$string` (`string`)
- `$flags` (`int`), default `0`, optional
- `$context` (`?resource`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `finfo_buffer` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/finfo_buffer.md).
