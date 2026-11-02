---
title: "gmp_hamdist()"
description: "Hamming distance between two GMP numbers."
sidebar:
  order: 605
---

## gmp_hamdist()

```php
function gmp_hamdist(object|string|int $num1, object|string|int $num2): int
```

Hamming distance between two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_hamdist.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_hamdist.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_hamdist` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_hamdist.md).
