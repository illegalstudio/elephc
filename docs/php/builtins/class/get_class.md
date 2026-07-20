---
title: "get_class()"
description: "Returns the name of the class of an object."
sidebar:
  order: 77
---

## get_class()

```php
function get_class(object $object = null): string
```

Returns the name of the class of an object.

**Parameters**:
- `$object` (`object`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_class.md).

