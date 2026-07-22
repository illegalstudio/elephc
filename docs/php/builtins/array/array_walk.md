---
title: "array_walk()"
description: "Applies a user function to every member of an array."
sidebar:
  order: 45
---

## array_walk()

```php
function array_walk(array $array, callable $callback): void
```

Applies a user function to every member of an array.

**Parameters**:
- `$array` (`array`), passed by reference
- `$callback` (`callable`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_walk.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_walk.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_walk` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_walk.md).
