---
title: "stream_set_blocking()"
description: "Sets blocking/non-blocking mode on a stream."
sidebar:
  order: 228
---

## stream_set_blocking()

```php
function stream_set_blocking(resource $stream, bool $enable): bool
```

Sets blocking/non-blocking mode on a stream.

**Parameters**:
- `$stream` (`resource`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_blocking.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_blocking.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_set_blocking` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_set_blocking.md).

