---
title: "gmp_neg()"
description: "Negates a GMP number."
sidebar:
  order: 616
---

## gmp_neg()

```php
function gmp_neg(object|string|int $num): object
```

Negates a GMP number.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_neg.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_neg.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_neg` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_neg.md).
