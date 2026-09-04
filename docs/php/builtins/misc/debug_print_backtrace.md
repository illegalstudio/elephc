---
title: "debug_print_backtrace()"
description: "Prints a PHP backtrace for the active call stack."
sidebar:
  order: 320
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/debug_print_backtrace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/debug_print_backtrace.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `debug_print_backtrace` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/debug_print_backtrace.md).
