---
title: "gmp_nextprime()"
description: "Returns the next prime greater than a GMP number."
sidebar:
  order: 617
---

## gmp_nextprime()

```php
function gmp_nextprime(object|string|int $num): object
```

Returns the next prime greater than a GMP number.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_nextprime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_nextprime.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_nextprime` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_nextprime.md).
