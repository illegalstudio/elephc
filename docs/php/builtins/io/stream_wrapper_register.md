---
title: "stream_wrapper_register()"
description: "Lowers `stream_wrapper_register(protocol, class, flags?)`."
sidebar:
  order: 209
---

## stream_wrapper_register()

```php
function stream_wrapper_register(string $protocol, string $class, int $flags): bool
```

Lowers `stream_wrapper_register(protocol, class, flags?)`.

**Parameters**:
- `$protocol` (`string`)
- `$class` (`string`)
- `$flags` (`int`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_wrapper_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_wrapper_register.md).

