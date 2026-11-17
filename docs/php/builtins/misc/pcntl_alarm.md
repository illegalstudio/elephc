---
title: "pcntl_alarm()"
description: "Schedules a SIGALRM and returns the prior alarm's remaining seconds."
---

## pcntl_alarm()

```php
function pcntl_alarm(int $seconds): int
```

Schedules a SIGALRM and returns the prior alarm's remaining seconds.

**Parameters**:
- `$seconds` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_alarm` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_alarm.md).
