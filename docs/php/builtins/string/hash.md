---
title: "hash()"
description: "Lowers `hash(algo, data, binary?)` through the shared runtime digest dispatcher."
sidebar:
  order: 332
---

## hash()

```php
function hash(string $algo, string $data, bool $binary = false, array $options = []): string
```

Lowers `hash(algo, data, binary?)` through the shared runtime digest dispatcher.

**Parameters**:
- `$algo` (`string`)
- `$data` (`string`)
- `$binary` (`bool`), default `false`, optional
- `$options` (`array`), default `[]`, optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash.md).

