---
title: "is_string()"
description: "Checks whether a variable is a string."
sidebar:
  order: 462
---

## is_string()

```php
function is_string(mixed $value): bool
```

Checks whether a variable is a string.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_string.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_string.md).
