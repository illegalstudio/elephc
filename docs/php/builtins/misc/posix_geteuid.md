---
title: "posix_geteuid()"
description: "Returns the effective user id of the current process."
---

## posix_geteuid()

```php
function posix_geteuid(): int
```

Returns the effective user id of the current process.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_geteuid` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_geteuid.md).
