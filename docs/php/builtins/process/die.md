---
title: "die()"
description: "Terminates execution with an optional status."
sidebar:
  order: 414
---

## die()

```php
function die(int $status = 0): void
```

Terminates execution with an optional status.

**Parameters**:
- `$status` (`int`), default `0`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through a dedicated compiler language-construct path.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/die.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/die.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `die` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/die.md).
