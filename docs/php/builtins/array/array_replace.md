---
title: "array_replace()"
description: "Replaces elements from passed arrays into the first array."
sidebar:
  order: 41
---

## array_replace()

```php
function array_replace(array $array, ...$replacements): array
```

Replaces elements from passed arrays into the first array.

**Parameters**:
- `$array` (`array`)
- `...$replacements` - variadic: collects excess arguments into `$replacements`.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_replace.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_replace.md).
