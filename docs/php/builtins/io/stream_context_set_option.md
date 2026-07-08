---
title: "stream_context_set_option()"
description: "Sets an option on the specified context."
sidebar:
  order: 196
---

## stream_context_set_option()

```php
function stream_context_set_option(resource $context, string $wrapper_or_options, string $option_name = null, mixed $value = null): bool
```

Sets an option on the specified context.

**Parameters**:
- `$context` (`resource`)
- `$wrapper_or_options` (`string`)
- `$option_name` (`string`), default `null`, optional
- `$value` (`mixed`), default `null`, optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_set_option` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_set_option.md).

