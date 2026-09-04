---
title: "bcfloor()"
description: "Rounds an arbitrary-precision decimal number down to an integer."
sidebar:
  order: 298
---

## bcfloor()

```php
function bcfloor(string $num): string
```

Rounds an arbitrary-precision decimal number down to an integer.

**Parameters**:
- `$num` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bcfloor.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bcfloor.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bcfloor` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bcfloor.md).
