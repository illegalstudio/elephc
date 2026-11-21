---
title: "proc_nice()"
description: "Changes the priority of the current process."
---

## proc_nice()

```php
function proc_nice(int $priority): bool
```

Changes the priority of the current process.

**Parameters**:
- `$priority` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_nice` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_nice.md).
