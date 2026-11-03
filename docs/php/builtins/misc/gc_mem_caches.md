---
title: "gc_mem_caches()"
description: "Drains runtime small-block allocator caches and returns the number of cached bytes released."
---

## gc_mem_caches()

```php
function gc_mem_caches(): int
```

Drains runtime small-block allocator caches and returns the number of cached bytes released.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_mem_caches` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_mem_caches.md).
