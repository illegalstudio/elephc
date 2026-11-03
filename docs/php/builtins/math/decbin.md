---
title: "decbin()"
description: "Converts an integer to its binary string representation."
sidebar:
  order: 595
---

## decbin()

```php
function decbin(int $num): string
```

Converts an integer to its binary string representation.

**Parameters**:
- `$num` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/decbin.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/decbin.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `decbin` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/decbin.md).
