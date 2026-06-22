---
title: "hash_update()"
description: "Lowers `hash_update(context, data)` through the incremental hash runtime helper."
sidebar:
  order: 339
---

## hash_update()

```php
function hash_update(resource $context, string $data): bool
```

Lowers `hash_update(context, data)` through the incremental hash runtime helper.

**Parameters**:
- `$context` (`resource`)
- `$data` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `hash_update` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/hash_update.md).

