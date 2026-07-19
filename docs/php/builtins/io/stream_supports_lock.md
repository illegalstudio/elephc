---
title: "stream_supports_lock()"
description: "Tells whether the stream supports locking."
sidebar:
  order: 229
---

## stream_supports_lock()

```php
function stream_supports_lock(resource $stream): bool
```

Tells whether the stream supports locking.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/stream_supports_lock.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stream_supports_lock.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_supports_lock` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_supports_lock.md).

