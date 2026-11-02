---
title: "gmp_or()"
description: "Bitwise OR of two GMP numbers."
sidebar:
  order: 618
---

## gmp_or()

```php
function gmp_or(object|string|int $num1, object|string|int $num2): object
```

Bitwise OR of two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_or.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_or.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_or` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_or.md).
