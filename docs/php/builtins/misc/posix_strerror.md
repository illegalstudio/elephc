---
title: "posix_strerror()"
description: "Returns the system error message associated with an errno value."
---

## posix_strerror()

```php
function posix_strerror(int $error_code): string
```

Returns the system error message associated with an errno value.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_strerror` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_strerror.md).
