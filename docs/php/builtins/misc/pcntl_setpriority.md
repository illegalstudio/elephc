---
title: "pcntl_setpriority()"
description: "Changes a process, process-group, or user scheduling priority."
---

## pcntl_setpriority()

```php
function pcntl_setpriority(int $priority, int $process_id = null, int $mode = 0): bool
```

Changes a process, process-group, or user scheduling priority.

**Parameters**:
- `$priority` (`int`)
- `$process_id` (`int`), default `null`, optional
- `$mode` (`int`), default `0`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_setpriority` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_setpriority.md).
