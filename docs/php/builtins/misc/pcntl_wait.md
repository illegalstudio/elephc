---
title: "pcntl_wait()"
description: "Waits for any child process and writes its target-native status."
---

## pcntl_wait()

```php
function pcntl_wait(mixed $status, int $flags = 0, mixed $resource_usage = []): int
```

Waits for any child process and writes its target-native status.

**Parameters**:
- `$status` (`mixed`), passed by reference
- `$flags` (`int`), default `0`, optional
- `$resource_usage` (`mixed`), passed by reference, default `[]`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wait` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wait.md).
