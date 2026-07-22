---
title: "stream_set_write_buffer()"
description: "Sets the write file buffering on a stream."
sidebar:
  order: 234
---

## stream_set_write_buffer()

```php
function stream_set_write_buffer(resource $stream, int $size): int
```

Sets the write file buffering on a stream.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_write_buffer.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_write_buffer.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_set_write_buffer` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_write_buffer.md).
