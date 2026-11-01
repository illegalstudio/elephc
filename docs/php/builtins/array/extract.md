---
title: "extract()"
description: "Imports array entries as variables in the current scope."
sidebar:
  order: 67
---

## extract()

```php
function extract(mixed $array, int $flags = 0, string $prefix = ''): int
```

Imports array entries as variables in the current scope.

**Parameters**:
- `$array` (`mixed`)
- `$flags` (`int`), default `0`, optional
- `$prefix` (`string`), default `''`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/array/extract.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/extract.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `extract` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/extract.md).
