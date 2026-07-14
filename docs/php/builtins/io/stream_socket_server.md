---
title: "stream_socket_server()"
description: "Create an Internet or Unix domain server socket."
sidebar:
  order: 227
---

## stream_socket_server()

```php
function stream_socket_server(string $address): mixed
```

Create an Internet or Unix domain server socket.

**Parameters**:
- `$address` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_server.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_server.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_server` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_server.md).

