---
title: "gmp_add()"
description: "Adds two GMP numbers."
sidebar:
  order: 590
---

## gmp_add()

```php
function gmp_add(object|string|int $num1, object|string|int $num2): object
```

Adds two GMP numbers.

**Parameters**:
- `$num1` (`object|string|int`)
- `$num2` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_add.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_add.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_add` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_add.md).
