---
title: "stream_is_local()"
description: "Checks if a stream is a local stream."
sidebar:
  order: 211
---

## stream_is_local()

```php
function stream_is_local(resource $stream): bool
```

Checks if a stream is a local stream.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/stream_is_local.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stream_is_local.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_is_local` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_is_local.md).

