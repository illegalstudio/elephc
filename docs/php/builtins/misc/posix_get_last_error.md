---
title: "posix_get_last_error()"
description: "Returns the errno recorded by the most recent failing POSIX operation."
---

## posix_get_last_error()

```php
function posix_get_last_error(): int
```

Returns the errno recorded by the most recent failing POSIX operation.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_get_last_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_get_last_error.md).
