---
title: "gmp_fact()"
description: "Computes a factorial."
sidebar:
  order: 602
---

## gmp_fact()

```php
function gmp_fact(object|string|int $num): object
```

Computes a factorial.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_fact.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_fact.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_fact` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_fact.md).
