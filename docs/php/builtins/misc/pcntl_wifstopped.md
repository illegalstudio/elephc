---
title: "pcntl_wifstopped()"
description: "Reports whether a child wait status represents a stopped process."
---

## pcntl_wifstopped()

```php
function pcntl_wifstopped(int $status): bool
```

Reports whether a child wait status represents a stopped process.

**Parameters**:
- `$status` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_wifstopped` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wifstopped.md).
