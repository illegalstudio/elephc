---
title: "spl_object_hash()"
description: "Return hash id for given object."
sidebar:
  order: 355
---

## spl_object_hash()

```php
function spl_object_hash(object $object): string
```

Return hash id for given object.

**Parameters**:
- `$object` (`object`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/spl_object_hash.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/spl_object_hash.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_object_hash` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_object_hash.md).
