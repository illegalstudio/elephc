---
title: "stream_select()"
description: "Runs the equivalent of the select() system call on the given arrays of streams."
sidebar:
  order: 229
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

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_select.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_select.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_select` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_select.md).
