---
title: "debug_backtrace()"
description: "Generates a PHP backtrace for the active call stack."
sidebar:
  order: 319
---

## debug_backtrace()

```php
function debug_backtrace(int $options = 1, int $limit = 0): array
```

Generates a PHP backtrace for the active call stack.

**Parameters**:
- `$options` (`int`), default `1`, optional
- `$limit` (`int`), default `0`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/debug_backtrace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/debug_backtrace.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `debug_backtrace` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/debug_backtrace.md).
