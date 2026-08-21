---
title: "hash_update()"
description: "Feeds data into an incremental hashing context. Provided by the compiler-injected hash prelude in compiled code."
sidebar:
  order: 423
---

## hash_update()

```php
function hash_update(HashContext $context, string $data): bool
```

Feeds data into an incremental hashing context. Provided by the compiler-injected hash prelude in compiled code.

**Parameters**:
- `$context` (`HashContext`)
- `$data` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through a compiler-injected PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_update.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_update` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_update.md).
