---
title: "simplexml_load_file()"
description: "Parses an XML file into a SimpleXML element."
sidebar:
  order: 1012
---

## simplexml_load_file()

```php
function simplexml_load_file(string $filename, ?string $class_name = 'SimpleXMLElement', int $options = 0, string $namespace_or_prefix = '', bool $is_prefix = false): SimpleXMLElement|false
```

Parses an XML file into a SimpleXML element.

**Parameters**:
- `$filename` (`string`)
- `$class_name` (`?string`), default `'SimpleXMLElement'`, optional
- `$options` (`int`), default `0`, optional
- `$namespace_or_prefix` (`string`), default `''`, optional
- `$is_prefix` (`bool`), default `false`, optional

**Returns**: `SimpleXMLElement|false`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/simplexml_load_file.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/simplexml_load_file.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `simplexml_load_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/simplexml_load_file.md).
