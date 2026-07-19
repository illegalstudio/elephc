---
title: "stream_socket_enable_crypto()"
description: "Turns encryption on/off on an already connected socket."
sidebar:
  order: 222
---

## stream_socket_enable_crypto()

```php
function stream_socket_enable_crypto(resource $stream, bool $enable, int $crypto_method = null, resource $session_stream = null): bool
```

Turns encryption on/off on an already connected socket.

**Parameters**:
- `$stream` (`resource`)
- `$enable` (`bool`)
- `$crypto_method` (`int`), default `null`, optional
- `$session_stream` (`resource`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_enable_crypto.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_enable_crypto.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_enable_crypto` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_enable_crypto.md).

