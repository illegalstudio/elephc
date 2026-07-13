---
title: "get_called_class()"
description: "get_called_class() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet."
sidebar:
  order: 76
---

## get_called_class()

```php
function get_called_class(): mixed
```

get_called_class() is available inside eval'd code via the magician interpreter; compiled (AOT) code does not support it yet.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: not available — compiled programs cannot call this builtin yet.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._
