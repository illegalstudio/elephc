---
title: "hash_update()"
description: "Pumps data into an active incremental hashing context."
sidebar:
  order: 378
---

## hash_update()

```php
function hash_update(resource $context, string $data): bool
```

Pumps data into an active incremental hashing context.

**Parameters**:
- `$context` (`resource`)
- `$data` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_update` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_update.md).
