---
title: "hash_file()"
description: "Lowers `hash_file(algo, filename, binary?)` by reading bytes then hashing them."
sidebar:
  order: 167
---

## hash_file()

```php
function hash_file(string $algo, string $filename, bool $binary = false, array $options = []): mixed
```

Lowers `hash_file(algo, filename, binary?)` by reading bytes then hashing them.

**Parameters**:
- `$algo` (`string`)
- `$filename` (`string`)
- `$binary` (`bool`), default `false`, optional
- `$options` (`array`), default `[]`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/hash_file.md).

