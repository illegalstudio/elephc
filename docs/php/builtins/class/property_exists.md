---
title: "property_exists()"
description: "Checks whether an object or class has a property."
sidebar:
  order: 89
---

## property_exists()

```php
function property_exists(mixed $object_or_class, string $property): bool
```

Checks whether an object or class has a property.

**Parameters**:
- `$object_or_class` (`mixed`)
- `$property` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/property_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/property_exists.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `property_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/property_exists.md).
