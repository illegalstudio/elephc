---
title: "stream_socket_pair()"
description: "Creates a pair of connected, indistinguishable socket streams."
sidebar:
  order: 224
---

## stream_socket_pair()

```php
function stream_socket_pair(int $domain, int $type, int $protocol): mixed
```

Creates a pair of connected, indistinguishable socket streams.

**Parameters**:
- `$domain` (`int`)
- `$type` (`int`)
- `$protocol` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_pair.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_pair.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_socket_pair` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_socket_pair.md).

