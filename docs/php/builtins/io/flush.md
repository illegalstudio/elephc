---
title: "flush()"
description: "Flushes the SAPI write buffer, which elephc does not have."
sidebar:
  order: 340
---

## flush()

```php
function flush(): void
```

Flushes the SAPI write buffer, which elephc does not have.

**Parameters**: none.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/flush.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/flush.md).
