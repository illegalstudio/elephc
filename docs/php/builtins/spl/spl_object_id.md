---
title: "spl_object_id()"
description: "Return the integer object handle for given object."
sidebar:
  order: 349
---

## spl_object_id()

```php
function spl_object_id(object $object): int
```

Return the integer object handle for given object.

**Parameters**:
- `$object` (`object`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_object_id.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_object_id.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_object_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_object_id.md).

