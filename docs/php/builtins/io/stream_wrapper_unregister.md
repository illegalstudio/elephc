---
title: "stream_wrapper_unregister()"
description: "Unregisters a previously registered URL wrapper."
sidebar:
  order: 245
---

## stream_wrapper_unregister()

```php
function stream_wrapper_unregister(string $protocol): bool
```

Unregisters a previously registered URL wrapper.

**Parameters**:
- `$protocol` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_unregister.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_unregister.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_wrapper_unregister` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_wrapper_unregister.md).

