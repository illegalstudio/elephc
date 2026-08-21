---
title: "hash_init()"
description: "Opens an incremental hashing context, returning a HashContext object. Provided by the compiler-injected hash prelude in compiled code; the eval interpreter still returns a resource."
sidebar:
  order: 422
---

## hash_init()

```php
function hash_init(string $algo): HashContext
```

Opens an incremental hashing context, returning a HashContext object. Provided by the compiler-injected hash prelude in compiled code; the eval interpreter still returns a resource.

**Parameters**:
- `$algo` (`string`)

**Returns**: `HashContext`

## Availability

- **Compiled (AOT)**: supported through a compiler-injected PHP prelude.
- **AOT signature compatibility**: `prelude-signature-subset` — compiled code accepts the signature shown above; eval may expose the broader canonical signature.
- **Effective eval signature**: `hash_init(string $algo, int $flags = 0, string $key = "")`.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/hash_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/hash_init.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_init.md).
