---
title: "stream_socket_client()"
description: "Open Internet or Unix domain socket connection."
sidebar:
  order: 255
---

## stream_socket_client()

```php
function stream_socket_client(string $address, mixed $error_code = null, mixed $error_message = null, mixed $timeout = null, int $flags = 4, mixed $context = null): mixed
```

Open Internet or Unix domain socket connection.

**Parameters**:
- `$address` (`string`)
- `$error_code` (`mixed`), passed by reference, default `null`, optional
- `$error_message` (`mixed`), passed by reference, default `null`, optional
- `$timeout` (`mixed`), default `null`, optional
- `$flags` (`int`), default `4`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_client.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_client.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_socket_client` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_client.md).
