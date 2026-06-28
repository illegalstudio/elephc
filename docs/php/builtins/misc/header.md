---
title: "header()"
description: "Lowers `header($line[, $replace[, $code]])` to `__rt_header`, materializing the"
sidebar:
  order: 260
---

## header()

```php
function header(mixed $header, mixed $replace, mixed $response_code): void
```

Lowers `header($line[, $replace[, $code]])` to `__rt_header`, materializing the

**Parameters**:
- `$header` (`mixed`)
- `$replace` (`mixed`), optional
- `$response_code` (`mixed`), optional

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `header` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/header.md).

