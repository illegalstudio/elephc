---
title: "pcntl_getpriority()"
description: "Returns a process, process-group, or user scheduling priority, or false on failure."
sidebar:
  order: 336
---

## pcntl_getpriority()

```php
function pcntl_getpriority(int $process_id = null, int $mode = 0): mixed
```

Returns a process, process-group, or user scheduling priority, or false on failure.

**Parameters**:
- `$process_id` (`int`), default `null`, optional
- `$mode` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getpriority.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/pcntl/pcntl_getpriority.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_getpriority` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_getpriority.md).
