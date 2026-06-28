---
title: "stream_select()"
description: "Lowers `stream_select(read, write, except, seconds, microseconds?)`."
sidebar:
  order: 193
---

## stream_select()

```php
function stream_select(array $read, array $write, array $except, int $seconds, int $microseconds): int
```

Lowers `stream_select(read, write, except, seconds, microseconds?)`.

**Parameters**:
- `$read` (`array`), passed by reference
- `$write` (`array`), passed by reference
- `$except` (`array`), passed by reference
- `$seconds` (`int`)
- `$microseconds` (`int`), optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_select` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_select.md).

