---
title: "gmp_scan1()"
description: "Finds the first set bit."
sidebar:
  order: 631
---

## gmp_scan1()

```php
function gmp_scan1(object|string|int $num1, int $start): int
```

Finds the first set bit.

**Parameters**:
- `$num1` (`object|string|int`)
- `$start` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_scan1.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_scan1.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_scan1` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_scan1.md).
