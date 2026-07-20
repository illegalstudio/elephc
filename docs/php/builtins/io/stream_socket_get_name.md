---
title: "stream_socket_get_name()"
description: "Retrieve the name of the local or remote sockets."
sidebar:
  order: 236
---

## stream_socket_get_name()

```php
function stream_socket_get_name(resource $socket, bool $remote): mixed
```

Retrieve the name of the local or remote sockets.

**Parameters**:
- `$socket` (`resource`)
- `$remote` (`bool`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_get_name.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_get_name.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_get_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_get_name.md).

