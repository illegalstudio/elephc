---
title: "hash_copy()"
description: "Copies the state of an incremental hashing context."
sidebar:
  order: 358
---

## hash_copy()

```php
function hash_copy(resource $context): mixed
```

Copies the state of an incremental hashing context.

**Parameters**:
- `$context` (`resource`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_copy.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_copy` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_copy.md).

