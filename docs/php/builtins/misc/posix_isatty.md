---
title: "posix_isatty()"
description: "Reports whether a file descriptor or stream refers to an interactive terminal."
---

## posix_isatty()

```php
function posix_isatty(mixed $file_descriptor): bool
```

Reports whether a file descriptor or stream refers to an interactive terminal.

**Parameters**:
- `$file_descriptor` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_isatty` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_isatty.md).
