---
title: "is_iterable()"
description: "Checks whether a variable is iterable."
sidebar:
  order: 454
---

## is_iterable()

```php
function is_iterable(mixed $value): bool
```

Checks whether a variable is iterable.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_iterable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_iterable.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_iterable` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_iterable.md).
