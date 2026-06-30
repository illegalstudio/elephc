---
title: "wordwrap()"
description: "Lowers `wordwrap(string, width?, break?, cut?)` through the shared runtime helper."
sidebar:
  order: 404
---

## wordwrap()

```php
function wordwrap(string $string, int $width, string $break, bool $cut_long_words): string
```

Lowers `wordwrap(string, width?, break?, cut?)` through the shared runtime helper.

**Parameters**:
- `$string` (`string`)
- `$width` (`int`), optional
- `$break` (`string`), optional
- `$cut_long_words` (`bool`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `wordwrap` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/wordwrap.md).

