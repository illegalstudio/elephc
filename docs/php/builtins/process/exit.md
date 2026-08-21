---
title: "exit()"
description: "Terminates execution with an optional status."
sidebar:
  order: 368
---

## exit()

```php
function exit(int $status = 0): void
```

Terminates execution with an optional status.

**Parameters**:
- `$status` (`int`), default `0`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through a dedicated compiler language-construct path.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/exit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/exit.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._
