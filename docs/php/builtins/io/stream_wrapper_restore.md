---
title: "stream_wrapper_restore()"
description: "Restores a previously unregistered built-in wrapper."
sidebar:
  order: 246
---

## stream_wrapper_restore()

```php
function stream_wrapper_restore(string $protocol): bool
```

Restores a previously unregistered built-in wrapper.

**Parameters**:
- `$protocol` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_restore.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_restore.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_wrapper_restore` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_wrapper_restore.md).
