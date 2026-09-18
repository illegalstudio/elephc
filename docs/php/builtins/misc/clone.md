---
title: "clone()"
description: "Creates a shallow clone, then initializes selected properties on the clone."
sidebar:
  order: 603
---

## clone()

```php
function clone(object $object, array $withProperties = []): object
```

Creates a shallow clone, then initializes selected properties on the clone.

**Parameters**:
- `$object` (`object`)
- `$withProperties` (`array`), default `[]`, optional

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/clone.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/clone.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `clone` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/clone.md).
