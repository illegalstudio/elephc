---
title: "hash_init()"
description: "Initialize an incremental hashing context."
sidebar:
  order: 353
---

## hash_init()

```php
function hash_init(string $algo, int $flags = 0, string $key = ''): mixed
```

Initialize an incremental hashing context.

**Parameters**:
- `$algo` (`string`)
- `$flags` (`int`), default `0`, optional
- `$key` (`string`), default `''`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_init.md).

