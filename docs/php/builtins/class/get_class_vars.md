---
title: "get_class_vars()"
description: "Returns visible default properties for a class."
sidebar:
  order: 87
---

## get_class_vars()

```php
function get_class_vars(mixed $class): mixed
```

Returns visible default properties for a class.

**Parameters**:
- `$class` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: not available — compiled programs cannot call this builtin (`eval-only-reflection`).
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_class_vars` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_class_vars.md).
