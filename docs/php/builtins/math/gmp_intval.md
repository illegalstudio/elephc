---
title: "gmp_intval()"
description: "Converts a GMP number to an integer."
sidebar:
  order: 608
---

## gmp_intval()

```php
function gmp_intval(object|string|int $num): int
```

Converts a GMP number to an integer.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_intval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_intval.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_intval` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_intval.md).
