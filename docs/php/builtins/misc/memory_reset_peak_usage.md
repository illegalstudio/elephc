---
title: "memory_reset_peak_usage()"
description: "Resets the live native heap peak to current usage. The reserved arena capacity and cumulative allocation/free counters are unchanged."
sidebar:
  order: 654
---

## memory_reset_peak_usage()

```php
function memory_reset_peak_usage(): void
```

Resets the live native heap peak to current usage. The reserved arena capacity and cumulative allocation/free counters are unchanged.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/memory_reset_peak_usage.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/memory_reset_peak_usage.rs)).

**Examples**:

```php
memory_reset_peak_usage();
$values = array_fill(0, 10000, 1);
echo memory_get_peak_usage();
```

## Internals

For how `memory_reset_peak_usage` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/memory_reset_peak_usage.md).
