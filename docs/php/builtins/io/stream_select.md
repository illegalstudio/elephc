---
title: "stream_select()"
description: "Runs the equivalent of the select() system call on the given arrays of streams."
sidebar:
  order: 210
---

## stream_select()

```php
function stream_select(array $read, array $write, array $except, int $seconds, int $microseconds = 0): int
```

Runs the equivalent of the select() system call on the given arrays of streams.

**Parameters**:
- `$read` (`array`), passed by reference
- `$write` (`array`), passed by reference
- `$except` (`array`), passed by reference
- `$seconds` (`int`)
- `$microseconds` (`int`), default `0`, optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_select` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_select.md).

