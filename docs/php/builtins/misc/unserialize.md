---
title: "unserialize()"
description: "Lowers `unserialize($data, $options?)` into the shared unserialize runtime helper."
sidebar:
  order: 282
---

## unserialize()

```php
function unserialize(mixed $data, mixed $options): mixed
```

Lowers `unserialize($data, $options?)` into the shared unserialize runtime helper.

**Parameters**:
- `$data` (`mixed`)
- `$options` (`mixed`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `unserialize` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/unserialize.md).

