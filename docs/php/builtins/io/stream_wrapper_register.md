---
title: "stream_wrapper_register()"
description: "Registers a URL wrapper implemented as a PHP class."
sidebar:
  order: 226
---

## stream_wrapper_register()

```php
function stream_wrapper_register(string $protocol, string $class, int $flags = 0): bool
```

Registers a URL wrapper implemented as a PHP class.

**Parameters**:
- `$protocol` (`string`)
- `$class` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_wrapper_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_wrapper_register.md).

