---
title: "pcntl_waitid()"
description: "Waits for a child state change and writes signal information plus optional PHP 8.5 resource usage."
---

## pcntl_waitid()

```php
function pcntl_waitid(int $idtype = 0, int $id = null, mixed $info = [], int $flags = 4, mixed $resource_usage = []): bool
```

Waits for a child state change and writes signal information plus optional PHP 8.5 resource usage.

**Parameters**:
- `$idtype` (`int`), default `0`, optional
- `$id` (`int`), default `null`, optional
- `$info` (`mixed`), passed by reference, default `[]`, optional
- `$flags` (`int`), default `4`, optional
- `$resource_usage` (`mixed`), passed by reference, default `[]`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `pcntl_waitid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/pcntl_waitid.md).
