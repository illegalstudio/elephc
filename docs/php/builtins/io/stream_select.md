---
title: "stream_select()"
description: "Runs the equivalent of the select() system call on the given arrays of streams, or returns false when a descriptor exceeds FD_SETSIZE."
---

## stream_select()

```php
function stream_select(array $read, array $write, array $except, int $seconds, int $microseconds = 0, mixed $context = null): int|false
```

Runs the equivalent of the select() system call on the given arrays of streams, or returns false when a descriptor exceeds FD_SETSIZE.

**Parameters**:
- `$read` (`array`), passed by reference
- `$write` (`array`), passed by reference
- `$except` (`array`), passed by reference
- `$seconds` (`int`)
- `$microseconds` (`int`), default `0`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_select` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_select.md).
