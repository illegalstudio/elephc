---
title: "is_object()"
description: "Checks whether a variable is an object."
sidebar:
  order: 433
---

## is_object()

```php
function is_object(mixed $value): bool
```

Checks whether a variable is an object.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_object.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_object.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_object` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_object.md).

