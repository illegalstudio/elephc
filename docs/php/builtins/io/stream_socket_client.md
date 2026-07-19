---
title: "stream_socket_client()"
description: "Open Internet or Unix domain socket connection."
sidebar:
  order: 234
---

## stream_socket_client()

```php
function stream_socket_client(string $address): mixed
```

Open Internet or Unix domain socket connection.

**Parameters**:
- `$address` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_client.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_client.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_client` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_client.md).

