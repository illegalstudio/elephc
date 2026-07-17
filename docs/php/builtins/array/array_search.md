---
title: "array_search()"
description: "Searches the array for a given value and returns the first corresponding key if successful."
sidebar:
  order: 35
---

## array_search()

```php
function array_search(mixed $needle, array $haystack, bool $strict = false): mixed
```

Searches the array for a given value and returns the first corresponding key if successful.

**Parameters**:
- `$needle` (`mixed`)
- `$haystack` (`array`)
- `$strict` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_search.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_search.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_search` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_search.md).

