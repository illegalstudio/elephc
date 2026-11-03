---
title: "pcntl_setcpuaffinity()"
description: "Changes the CPU affinity mask for a Linux process."
---

## pcntl_setcpuaffinity()

```php
function pcntl_setcpuaffinity(int $process_id = null, mixed $cpu_ids = []): bool
```

Changes the CPU affinity mask for a Linux process.

**Parameters**:
- `$process_id` (`int`), default `null`, optional
- `$cpu_ids` (`mixed`), default `[]`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setcpuaffinity` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setcpuaffinity.md).
