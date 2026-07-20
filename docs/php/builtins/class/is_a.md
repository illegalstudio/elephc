---
title: "is_a()"
description: "Checks whether an object is of a given type or has it as one of its parents."
sidebar:
  order: 86
---

## is_a()

```php
function is_a(object $object_or_class, string $class, bool $allow_string = false): bool
```

Checks whether an object is of a given type or has it as one of its parents.

**Parameters**:
- `$object_or_class` (`object`)
- `$class` (`string`)
- `$allow_string` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/is_a.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/is_a.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_a` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/is_a.md).

