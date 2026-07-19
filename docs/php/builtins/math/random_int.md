---
title: "random_int()"
description: "Get a cryptographically secure, uniformly selected integer."
sidebar:
  order: 268
---

## random_int()

```php
function random_int(int $min, int $max): int
```

Get a cryptographically secure, uniformly selected integer.

**Parameters**:
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/random_int.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/random_int.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `random_int` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/random_int.md).

