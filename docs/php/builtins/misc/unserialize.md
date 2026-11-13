---
title: "unserialize()"
description: "Creates a PHP value from a stored representation."
---

## unserialize()

```php
function unserialize(string $data, mixed $options = []): mixed
```

Creates a PHP value from a stored representation.

**Parameters**:
- `$data` (`string`)
- `$options` (`mixed`), default `[]`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `unserialize` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/unserialize.md).
