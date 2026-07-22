---
title: "stream_set_chunk_size()"
description: "Sets the read chunk size on a stream."
sidebar:
  order: 231
---

## stream_set_chunk_size()

```php
function stream_set_chunk_size(resource $stream, int $size): int
```

Sets the read chunk size on a stream.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_chunk_size.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_chunk_size.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_set_chunk_size` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_chunk_size.md).
