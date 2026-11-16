---
title: "pcntl_errno()"
description: "Returns the errno recorded by the most recent failing PCNTL operation."
---

## pcntl_errno()

```php
function pcntl_errno(): int
```

Returns the errno recorded by the most recent failing PCNTL operation.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_errno` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_errno.md).
