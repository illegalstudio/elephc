---
title: "get_class_vars()"
description: "get_class_vars() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet."
sidebar:
  order: 79
---

## get_class_vars()

```php
function get_class_vars(mixed $class): mixed
```

get_class_vars() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet.

**Parameters**:
- `$class` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: not available — compiled programs cannot call this builtin yet.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_vars.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._
