---
title: "array_pad()"
description: "Pads an array to the specified length with a value."
sidebar:
  order: 26
---

## array_pad()

```php
function array_pad(array $array, int $length, mixed $value): array
```

Pads an array to the specified length with a value.

**Parameters**:
- `$array` (`array`)
- `$length` (`int`)
- `$value` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/array_pad.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_pad.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `array_pad` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/array_pad.md).
