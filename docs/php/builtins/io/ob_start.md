---
title: "ob_start()"
description: "Turns on output buffering."
---

## ob_start()

```php
function ob_start(mixed $callback = null, int $chunk_size = 0, int $flags = 112): bool
```

Turns on output buffering.

**Parameters**:
- `$callback` (`mixed`), default `null`, optional
- `$chunk_size` (`int`), default `0`, optional
- `$flags` (`int`), default `112`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ob_start` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_start.md).
