---
title: "pcntl_fork()"
description: "Forks the current process and returns the child or parent process identifier."
---

## pcntl_fork()

```php
function pcntl_fork(): int
```

Forks the current process and returns the child or parent process identifier.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_fork` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_fork.md).
