---
title: "getrandmax()"
description: "Show largest possible random value."
sidebar:
  order: 579
---

## getrandmax()

```php
function getrandmax(): int
```

Show largest possible random value.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/getrandmax.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/getrandmax.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getrandmax` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/getrandmax.md).
