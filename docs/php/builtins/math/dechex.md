---
title: "dechex()"
description: "Converts an integer to its hexadecimal string representation."
sidebar:
  order: 598
---

## dechex()

```php
function dechex(int $num): string
```

Converts an integer to its hexadecimal string representation.

**Parameters**:
- `$num` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/dechex.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/dechex.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `dechex` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/dechex.md).
