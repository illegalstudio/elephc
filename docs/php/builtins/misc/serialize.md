---
title: "serialize()"
description: "Generates a storable representation of a value."
sidebar:
  order: 757
---

## serialize()

```php
function serialize(mixed $value): string
```

Generates a storable representation of a value.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/serialize.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/serialize.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `serialize` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/serialize.md).
