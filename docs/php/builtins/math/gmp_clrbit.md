---
title: "gmp_clrbit()"
description: "Clears a bit in a GMP number."
sidebar:
  order: 593
---

## gmp_clrbit()

```php
function gmp_clrbit(object $num, int $index): void
```

Clears a bit in a GMP number.

**Parameters**:
- `$num` (`object`)
- `$index` (`int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_clrbit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_clrbit.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_clrbit` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_clrbit.md).
