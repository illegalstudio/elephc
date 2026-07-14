---
title: "settype()"
description: "Sets the type of a variable."
sidebar:
  order: 436
---

## settype()

```php
function settype(mixed $var, string $type): bool
```

Sets the type of a variable.

**Parameters**:
- `$var` (`mixed`), passed by reference
- `$type` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/settype.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/settype.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `settype` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/settype.md).

