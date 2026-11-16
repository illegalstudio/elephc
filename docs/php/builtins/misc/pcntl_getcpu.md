---
title: "pcntl_getcpu()"
description: "Returns the logical CPU on which the calling thread is executing."
---

## pcntl_getcpu()

```php
function pcntl_getcpu(): int
```

Returns the logical CPU on which the calling thread is executing.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_getcpu` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_getcpu.md).
