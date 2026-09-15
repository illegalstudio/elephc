---
title: "array_flip()"
description: "Exchanges keys and values. Declared PHP arrays use runtime value tags, warn and skip values other than integers or strings, and keep the last key for duplicate values. Concrete AOT storage remains restricted to integer or string elements."
sidebar:
  order: 14
---

## array_flip()

```php
function array_flip(array $array): array
```

Exchanges keys and values. Declared PHP arrays use runtime value tags, warn and skip values other than integers or strings, and keep the last key for duplicate values. Concrete AOT storage remains restricted to integer or string elements.

**Parameters**:
- `$array` (`array`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_flip.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_flip.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `array_flip` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_flip.md).
