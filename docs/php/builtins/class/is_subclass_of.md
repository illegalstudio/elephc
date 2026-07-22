---
title: "is_subclass_of()"
description: "Checks if the object has a given class as one of its parents or implements it."
sidebar:
  order: 87
---

## is_subclass_of()

```php
function is_subclass_of(mixed $object_or_class, string $class, bool $allow_string = true): bool
```

Checks if the object has a given class as one of its parents or implements it.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$class` (`string`)
- `$allow_string` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/is_subclass_of.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/is_subclass_of.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_subclass_of` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/is_subclass_of.md).
