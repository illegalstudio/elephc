---
title: "in_array()"
description: "Checks if a value exists in an array."
sidebar:
  order: 52
---

## in_array()

```php
function in_array(mixed $needle, array $haystack, bool $strict = false): bool
```

Checks if a value exists in an array.

**Parameters**:
- `$needle` (`mixed`)
- `$haystack` (`array`)
- `$strict` (`bool`), default `false`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/in_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/in_array.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `in_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/in_array.md).

