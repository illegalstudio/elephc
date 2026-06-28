---
title: "stream_context_set_option()"
description: "Lowers `stream_context_set_option(context, options)` and the four-argument form."
sidebar:
  order: 179
---

## stream_context_set_option()

```php
function stream_context_set_option(resource $context, string $wrapper_or_options, string $option_name, mixed $value): bool
```

Lowers `stream_context_set_option(context, options)` and the four-argument form.

**Parameters**:
- `$context` (`resource`)
- `$wrapper_or_options` (`string`)
- `$option_name` (`string`), optional
- `$value` (`mixed`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_set_option` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_set_option.md).

