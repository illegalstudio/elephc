---
title: "array_replace_recursive()"
description: "Replaces elements from passed arrays into the first array recursively."
sidebar:
  order: 42
---

## array_replace_recursive()

```php
function array_replace_recursive(array $array, ...$replacements): array
```

Replaces elements from passed arrays into the first array recursively.

**Parameters**:
- `$array` (`array`)
- `...$replacements` - variadic: collects excess arguments into `$replacements`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_recursive.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_recursive.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_replace_recursive` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_replace_recursive.md).
