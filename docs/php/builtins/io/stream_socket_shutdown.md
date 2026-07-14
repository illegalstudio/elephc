---
title: "stream_socket_shutdown()"
description: "Shutdown a full-duplex connection."
sidebar:
  order: 228
---

## stream_socket_shutdown()

```php
function stream_socket_shutdown(resource $stream, int $mode): bool
```

Shutdown a full-duplex connection.

**Parameters**:
- `$stream` (`resource`)
- `$mode` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_shutdown.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_shutdown.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_shutdown` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_shutdown.md).

