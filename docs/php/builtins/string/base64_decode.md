---
title: "base64_decode()"
description: "Lowers a one-argument string builtin that directly delegates to a runtime helper."
sidebar:
  order: 335
---

## base64_decode()

```php
function base64_decode(string $string, bool $strict): string
```

Lowers a one-argument string builtin that directly delegates to a runtime helper.

**Parameters**:
- `$string` (`string`)
- `$strict` (`bool`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `base64_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/base64_decode.md).

