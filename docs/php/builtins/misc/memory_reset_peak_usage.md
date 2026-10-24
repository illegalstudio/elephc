---
title: "memory_reset_peak_usage()"
description: "Resets the peak memory counters to the current usage."
sidebar:
  order: 640
---

## memory_reset_peak_usage()

```php
function memory_reset_peak_usage(): void
```

Resets the peak memory counters to the current usage.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/memory_reset_peak_usage.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/memory_reset_peak_usage.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `memory_reset_peak_usage` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/memory_reset_peak_usage.md).
