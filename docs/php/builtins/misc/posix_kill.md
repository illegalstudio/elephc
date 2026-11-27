---
title: "posix_kill()"
description: "Sends a signal to a process or process group."
---

## posix_kill()

```php
function posix_kill(int $process_id, int $signal): bool
```

Sends a signal to a process or process group.

**Parameters**:
- `$process_id` (`int`)
- `$signal` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_kill` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_kill.md).
