---
title: "gc_status()"
description: "Returns live collector counters, roots, and phase timings in PHP's array shape. Elephc has no collector buffer, so full, threshold, and buffer_size are compatibility fields fixed to false, 0, and 0."
---

## gc_status()

```php
function gc_status(): mixed
```

Returns live collector counters, roots, and phase timings in PHP's array shape. Elephc has no collector buffer, so full, threshold, and buffer_size are compatibility fields fixed to false, 0, and 0.

**Parameters**: none.

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gc_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/gc_status.md).
