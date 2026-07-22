---
title: "is_callable()"
description: "Checks whether a variable can be called as a function."
sidebar:
  order: 442
---

## is_callable()

```php
function is_callable(mixed $value): bool
```

Checks whether a variable can be called as a function.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/is_callable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/is_callable.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_callable` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_callable.md).
