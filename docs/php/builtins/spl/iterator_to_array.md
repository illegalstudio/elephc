---
title: "iterator_to_array()"
description: "Copy the iterator into an array."
sidebar:
  order: 327
---

## iterator_to_array()

```php
function iterator_to_array(traversable $iterator, bool $preserve_keys = true): array
```

Copy the iterator into an array.

**Parameters**:
- `$iterator` (`traversable`)
- `$preserve_keys` (`bool`), default `true`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/iterator_to_array.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/iterator_to_array.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `iterator_to_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/iterator_to_array.md).

