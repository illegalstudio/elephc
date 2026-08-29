---
title: "pcntl_getcpuaffinity()"
description: "Returns the CPU affinity mask for a Linux process, or false on failure."
sidebar:
  order: 334
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getcpuaffinity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getcpuaffinity.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_getcpuaffinity` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_getcpuaffinity.md).
