---
title: "gmp_cmp()"
description: "Compares two GMP numbers."
sidebar:
  order: 594
---

## gmp_cmp()

```php
function gmp_cmp(object|string|int $num1, object|string|int $num2): int
```

Compares two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_cmp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_cmp.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_cmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_cmp.md).
