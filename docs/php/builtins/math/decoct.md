---
title: "decoct()"
description: "Converts an integer to its octal string representation."
sidebar:
  order: 597
---

## decoct()

```php
function decoct(int $num): string
```

Converts an integer to its octal string representation.

**Parameters**:
- `$num` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/decoct.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/decoct.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `decoct` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/decoct.md).
