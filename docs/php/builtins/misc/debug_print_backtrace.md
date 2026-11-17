---
title: "debug_print_backtrace()"
description: "Prints a PHP backtrace for the active call stack."
---

## debug_print_backtrace()

```php
function debug_print_backtrace(int $options = 0, int $limit = 0): void
```

Prints a PHP backtrace for the active call stack.

**Parameters**:
- `$options` (`int`), default `0`, optional
- `$limit` (`int`), default `0`, optional

**Returns**: `void`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `debug_print_backtrace` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/debug_print_backtrace.md).
