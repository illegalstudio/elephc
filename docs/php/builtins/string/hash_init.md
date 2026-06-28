---
title: "hash_init()"
description: "Lowers `hash_init(algo)` and returns a boxed HashContext resource."
sidebar:
  order: 336
---

## hash_init()

```php
function hash_init(string $algo, int $flags = 0, string $key = '', array $options = []): mixed
```

Lowers `hash_init(algo)` and returns a boxed HashContext resource.

**Parameters**:
- `$algo` (`string`)
- `$flags` (`int`), default `0`, optional
- `$key` (`string`), default `''`, optional
- `$options` (`array`), default `[]`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_init.md).

