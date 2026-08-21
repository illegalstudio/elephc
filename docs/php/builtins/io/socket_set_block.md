---
title: "socket_set_block()"
description: "Set blocking mode on a socket stream (alias of stream_set_blocking)."
sidebar:
  order: 222
---

## socket_set_block()

```php
function socket_set_block(mixed $stream, bool $enable): bool
```

Set blocking mode on a socket stream (alias of stream_set_blocking).

**Parameters**:
- `$stream` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/socket_set_block.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/socket_set_block.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `socket_set_block` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/socket_set_block.md).
