---
title: "get_class_methods()"
description: "get_class_methods() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet."
sidebar:
  order: 78
---

## get_class_methods()

```php
function get_class_methods(mixed $object_or_class): mixed
```

get_class_methods() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet.

**Parameters**:
- `$object_or_class` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: not available — compiled programs cannot call this builtin yet.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._
