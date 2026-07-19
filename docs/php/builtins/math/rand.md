---
title: "rand()"
description: "Generate a random integer."
sidebar:
  order: 267
---

## rand()

```php
function rand(int $min, int $max): int
```

Generate a random integer.

**Parameters**:
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/rand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/rand.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/rand.md).

