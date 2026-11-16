---
title: "pcntl_wifsignaled()"
description: "Reports whether a child wait status represents signal termination."
---

## pcntl_wifsignaled()

```php
function pcntl_wifsignaled(int $status): bool
```

Reports whether a child wait status represents signal termination.

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

For how `pcntl_wifsignaled` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_wifsignaled.md).
