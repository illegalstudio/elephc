---
title: "get_class_vars()"
description: "Returns visible default properties for a class. AOT supports direct calls, literal call_user_func calls, and first-class callables; runtime-selected callable targets are unsupported."
sidebar:
  order: 86
---

## get_class_vars()

```php
function get_class_vars(mixed $class): array
```

Returns visible default properties for a class. AOT supports direct calls, literal call_user_func calls, and first-class callables; runtime-selected callable targets are unsupported.

**Parameters**:
- `$class` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_class_vars` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_class_vars.md).
