---
title: "constant()"
description: "Returns the value of a constant given its name."
sidebar:
  order: 330
---

## constant()

```php
function constant(string $name): mixed
```

Returns the value of a constant given its name.

**Parameters**:
- `$name` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/constant.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/constant.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `constant` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/constant.md).
