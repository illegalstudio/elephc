---
title: "memory_reset_peak_usage()"
description: "Resets the live native heap peak to current usage. The reserved arena capacity and cumulative allocation/free counters are unchanged."
---

## memory_reset_peak_usage()

```php
function memory_reset_peak_usage(): void
```

Resets the live native heap peak to current usage. The reserved arena capacity and cumulative allocation/free counters are unchanged.

**Parameters**: none.

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

**Examples**:

```php
memory_reset_peak_usage();
$values = array_fill(0, 10000, 1);
echo memory_get_peak_usage();
```

## Internals

For how `memory_reset_peak_usage` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/memory_reset_peak_usage.md).
