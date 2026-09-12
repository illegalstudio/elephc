---
title: "get_class_vars()"
description: "Returns visible default properties for a class, excluding virtual properties. Uninitialized backed properties are returned as null. AOT supports direct calls, literal call_user_func calls, first-class callables, and argument unpacking, with a class-name string that may be a boxed runtime value; a non-string runtime tag throws TypeError. Runtime-selected callable targets are unsupported."
sidebar:
  order: 86
---

## get_class_vars()

```php
function get_class_vars(mixed $class): array
```

Returns visible default properties for a class, excluding virtual properties. Uninitialized backed properties are returned as null. AOT supports direct calls, literal call_user_func calls, first-class callables, and argument unpacking, with a class-name string that may be a boxed runtime value; a non-string runtime tag throws TypeError. Runtime-selected callable targets are unsupported.

**Parameters**:
- `$class` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_class_vars` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_class_vars.md).
