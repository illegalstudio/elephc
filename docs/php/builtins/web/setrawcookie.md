---
title: "setrawcookie()"
description: "Implemented by the compiler-injected web prelude."
sidebar:
  order: 874
---

## setrawcookie()

```php
function setrawcookie(mixed $name, mixed $value = '', mixed $expires = 0, mixed $path = '', mixed $domain = '', mixed $secure = false, mixed $httponly = false): mixed
```

Implemented by the compiler-injected web prelude.

**Parameters**:
- `$name` (`mixed`)
- `$value` (`mixed`), default `''`, optional
- `$expires` (`mixed`), default `0`, optional
- `$path` (`mixed`), default `''`, optional
- `$domain` (`mixed`), default `''`, optional
- `$secure` (`mixed`), default `false`, optional
- `$httponly` (`mixed`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `setrawcookie` is implemented in the compiler, see [the internals page](../../../internals/builtins/web/setrawcookie.md).
