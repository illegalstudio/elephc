---
title: "get_parent_class()"
description: "Returns the name of the parent class of an object or class."
sidebar:
  order: 84
---

## get_parent_class()

```php
function get_parent_class(mixed $object_or_class = null): string
```

Returns the name of the parent class of an object or class.

**Parameters**:
- `$object_or_class` (`mixed`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_parent_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_parent_class.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_parent_class` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_parent_class.md).
