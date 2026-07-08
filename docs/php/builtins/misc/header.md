---
title: "header()"
description: "Sends a raw HTTP header."
sidebar:
  order: 275
---

## header()

```php
function header(string $header, bool $replace = true, int $response_code = 0): void
```

Sends a raw HTTP header.

**Parameters**:
- `$header` (`string`)
- `$replace` (`bool`), default `true`, optional
- `$response_code` (`int`), default `0`, optional

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `header` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/header.md).

