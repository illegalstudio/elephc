---
title: "die()"
description: "die() — process builtin supported by Elephc."
sidebar:
  order: 323
---

## die()

```php
function die(int $status): void
```

`die()` is a process builtin supported by Elephc. Behavior matches the PHP manual unless noted below.

**Parameters**:
- `$status` (`int`), optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/die.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/die.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._
