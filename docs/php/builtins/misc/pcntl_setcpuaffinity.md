---
title: "pcntl_setcpuaffinity()"
description: "Changes the CPU affinity mask for a Linux process."
sidebar:
  order: 337
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setcpuaffinity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_setcpuaffinity.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setcpuaffinity` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setcpuaffinity.md).
