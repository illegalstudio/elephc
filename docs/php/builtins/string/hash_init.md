---
title: "hash_init()"
description: "Opens an incremental hashing context, returning a HashContext object. Provided by the compiler-injected hash prelude in compiled code; the eval interpreter still returns a resource."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code accepts the signature shown above; eval() accepts the broader `hash_init(string $algo, int $flags = 0, string $key = "")`.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hash_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_init.md).
