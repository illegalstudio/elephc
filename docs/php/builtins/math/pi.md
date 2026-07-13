---
title: "pi()"
description: "Gets value of pi."
sidebar:
  order: 264
---

## pi()

```php
function pi(): float
```

Gets value of pi.

**Parameters**: none.

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/pi.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/pi.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `pi` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/pi.md).

