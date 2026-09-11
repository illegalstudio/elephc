---
title: "get_class_methods()"
description: "Returns visible PHP method names, excluding generated property-hook accessors. AOT supports direct calls, literal call_user_func calls, first-class callables, and argument unpacking, with an object, a class-name string, or a boxed value whose runtime tag is an object or a string; any other tag throws TypeError. Runtime-selected callable targets are unsupported."
sidebar:
  order: 85
---

## get_class_methods()

```php
function get_class_methods(mixed $object_or_class): array
```

Returns visible PHP method names, excluding generated property-hook accessors. AOT supports direct calls, literal call_user_func calls, first-class callables, and argument unpacking, with an object, a class-name string, or a boxed value whose runtime tag is an object or a string; any other tag throws TypeError. Runtime-selected callable targets are unsupported.

**Parameters**:
- `$object_or_class` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_class_methods` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_class_methods.md).
