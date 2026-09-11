---
title: "sizeof()"
description: "Alias of count()."
sidebar:
  order: 67
---

## sizeof()

```php
function sizeof(mixed $value, int $mode = 0): int
```

Alias of count().

**Parameters**:
- `$value` (`mixed`)
- `$mode` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/sizeof.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/sizeof.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `sizeof` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/sizeof.md).
