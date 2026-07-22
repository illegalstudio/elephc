---
title: "stream_set_timeout()"
description: "Sets timeout period on a stream."
sidebar:
  order: 233
---

## stream_set_timeout()

```php
function stream_set_timeout(resource $stream, int $seconds, int $microseconds = 0): bool
```

Sets timeout period on a stream.

**Parameters**:
- `$stream` (`resource`)
- `$seconds` (`int`)
- `$microseconds` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_timeout.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_timeout.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_set_timeout` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_timeout.md).
