---
title: "hash_file()"
description: "Generates a hash value using the contents of a given file."
sidebar:
  order: 184
---

## hash_file()

```php
function hash_file(string $algo, string $filename, bool $binary = false): mixed
```

Generates a hash value using the contents of a given file.

**Parameters**:
- `$algo` (`string`)
- `$filename` (`string`)
- `$binary` (`bool`), default `false`, optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/hash_file.md).

