---
title: "get_object_vars()"
description: "get_object_vars() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet."
sidebar:
  order: 83
---

## get_object_vars()

```php
function get_object_vars(mixed $object): mixed
```

get_object_vars() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: not available — compiled programs cannot call this builtin yet.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_object_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_object_vars.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._
