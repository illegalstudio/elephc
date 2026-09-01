---
title: "bcceil()"
description: "Rounds an arbitrary-precision decimal number up to an integer."
sidebar:
  order: 269
---

## bcceil()

```php
function bcceil(string $num): string
```

Rounds an arbitrary-precision decimal number up to an integer.

**Parameters**:
- `$num` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcceil.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcceil.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcceil` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcceil.md).
