---
title: "get_class_methods()"
description: "Returns method names visible on an object or class."
sidebar:
  order: 86
---

## get_class_methods()

```php
function get_class_methods(mixed $object_or_class): mixed
```

Returns method names visible on an object or class.

**Parameters**:
- `$object_or_class` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: not available — compiled programs cannot call this builtin (`eval-only-reflection`).
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_class_methods` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_class_methods.md).
