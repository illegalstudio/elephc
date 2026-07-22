---
title: "stream_filter_remove()"
description: "Removes a filter from a stream."
sidebar:
  order: 219
---

## stream_filter_remove()

```php
function stream_filter_remove(resource $stream_filter): bool
```

Removes a filter from a stream.

**Parameters**:
- `$stream_filter` (`resource`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_remove.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_remove.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_filter_remove` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_filter_remove.md).
