---
title: "pcntl_getcpuaffinity()"
description: "Returns the CPU affinity mask for a Linux process, or false on failure."
---

## pcntl_getcpuaffinity()

```php
function pcntl_getcpuaffinity(int $process_id = null): mixed
```

Returns the CPU affinity mask for a Linux process, or false on failure.

**Parameters**:
- `$process_id` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_getcpuaffinity` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_getcpuaffinity.md).
