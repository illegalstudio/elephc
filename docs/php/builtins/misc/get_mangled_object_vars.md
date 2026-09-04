---
title: "get_mangled_object_vars()"
description: "Returns an object's properties using PHP's visibility-mangled keys."
sidebar:
  order: 341
---

## get_mangled_object_vars()

```php
function get_mangled_object_vars(mixed $object): array
```

Returns an object's properties using PHP's visibility-mangled keys.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/get_mangled_object_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/get_mangled_object_vars.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_mangled_object_vars` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_mangled_object_vars.md).
