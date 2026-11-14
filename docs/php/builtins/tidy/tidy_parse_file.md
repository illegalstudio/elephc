---
title: "tidy_parse_file()"
description: "Parses an HTML file, optionally searching the include path, and returns a tidy document object."
---

## tidy_parse_file()

```php
function tidy_parse_file(string $filename, array|string|null $config = null, ?string $encoding = null, bool $useIncludePath = false): mixed
```

Parses an HTML file, optionally searching the include path, and returns a tidy document object.

**Parameters**:
- `$filename` (`string`)
- `$config` (`array|string|null`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional
- `$useIncludePath` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_parse_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_parse_file.md).
