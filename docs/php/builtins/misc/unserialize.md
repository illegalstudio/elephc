---
title: "unserialize()"
description: "Creates a PHP value from a stored representation."
sidebar:
  order: 301
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `unserialize` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/unserialize.md).
