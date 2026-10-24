---
title: "memory_get_peak_usage()"
description: "Returns the peak managed-heap usage in bytes: the live payload-plus-header high watermark, or with $real_usage the highest bump offset the heap allocator has carved from its arena."
sidebar:
  order: 638
---

## memory_get_peak_usage()

```php
function memory_get_peak_usage(bool $real_usage = false): int
```

Returns the peak managed-heap usage in bytes: the live payload-plus-header high watermark, or with $real_usage the highest bump offset the heap allocator has carved from its arena.

**Parameters**:
- `$real_usage` (`bool`), default `false`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/memory_get_peak_usage.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/memory_get_peak_usage.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `memory_get_peak_usage` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/memory_get_peak_usage.md).
