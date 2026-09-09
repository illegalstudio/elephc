---
title: "XML builtins"
description: "Builtins in the XML category."
sidebar:
  order: 122
---

## XML builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`xml_error_string()`](./xml/xml_error_string.md) | `(int $error_code): ?string` | `?string` | ✓ | ✓ |
| [`xml_get_current_byte_index()`](./xml/xml_get_current_byte_index.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_get_current_column_number()`](./xml/xml_get_current_column_number.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_get_current_line_number()`](./xml/xml_get_current_line_number.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_get_error_code()`](./xml/xml_get_error_code.md) | `(mixed $parser): int` | `int` | ✓ | ✓ |
| [`xml_parse()`](./xml/xml_parse.md) | `(mixed $parser, string $data, bool $is_final = false): int` | `int` | ✓ | ✓ |
| [`xml_parse_into_struct()`](./xml/xml_parse_into_struct.md) | `(mixed $parser, string $data, mixed $values, mixed $index = null): int` | `int` | ✓ | ✓ |
| [`xml_parser_create()`](./xml/xml_parser_create.md) | `(?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`xml_parser_create_ns()`](./xml/xml_parser_create_ns.md) | `(?string $encoding = null, string $separator = ':'): mixed` | `mixed` | ✓ | ✓ |
| [`xml_parser_free()`](./xml/xml_parser_free.md) | `(mixed $parser): bool` | `bool` | ✓ | ✓ |
| [`xml_parser_get_option()`](./xml/xml_parser_get_option.md) | `(mixed $parser, int $option): mixed` | `mixed` | ✓ | ✓ |
| [`xml_parser_set_option()`](./xml/xml_parser_set_option.md) | `(mixed $parser, int $option, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`xml_set_character_data_handler()`](./xml/xml_set_character_data_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_default_handler()`](./xml/xml_set_default_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_element_handler()`](./xml/xml_set_element_handler.md) | `(mixed $parser, mixed $start_handler, mixed $end_handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_end_namespace_decl_handler()`](./xml/xml_set_end_namespace_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_external_entity_ref_handler()`](./xml/xml_set_external_entity_ref_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_notation_decl_handler()`](./xml/xml_set_notation_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_object()`](./xml/xml_set_object.md) | `(mixed $parser, mixed $object): bool` | `bool` | ✓ | ✓ |
| [`xml_set_processing_instruction_handler()`](./xml/xml_set_processing_instruction_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_start_namespace_decl_handler()`](./xml/xml_set_start_namespace_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xml_set_unparsed_entity_decl_handler()`](./xml/xml_set_unparsed_entity_decl_handler.md) | `(mixed $parser, mixed $handler): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_attribute()`](./xml/xmlwriter_end_attribute.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_cdata()`](./xml/xmlwriter_end_cdata.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_comment()`](./xml/xmlwriter_end_comment.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_document()`](./xml/xmlwriter_end_document.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd()`](./xml/xmlwriter_end_dtd.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd_attlist()`](./xml/xmlwriter_end_dtd_attlist.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd_element()`](./xml/xmlwriter_end_dtd_element.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_dtd_entity()`](./xml/xmlwriter_end_dtd_entity.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_element()`](./xml/xmlwriter_end_element.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_end_pi()`](./xml/xmlwriter_end_pi.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_flush()`](./xml/xmlwriter_flush.md) | `(mixed $writer, bool $empty = true): mixed` | `mixed` | ✓ | ✓ |
| [`xmlwriter_full_end_element()`](./xml/xmlwriter_full_end_element.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_open_memory()`](./xml/xmlwriter_open_memory.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`xmlwriter_open_uri()`](./xml/xmlwriter_open_uri.md) | `(string $uri): mixed` | `mixed` | ✓ | ✓ |
| [`xmlwriter_output_memory()`](./xml/xmlwriter_output_memory.md) | `(mixed $writer, bool $flush = true): string` | `string` | ✓ | ✓ |
| [`xmlwriter_set_indent()`](./xml/xmlwriter_set_indent.md) | `(mixed $writer, bool $enable): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_set_indent_string()`](./xml/xmlwriter_set_indent_string.md) | `(mixed $writer, string $indentation): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_attribute()`](./xml/xmlwriter_start_attribute.md) | `(mixed $writer, string $name): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_attribute_ns()`](./xml/xmlwriter_start_attribute_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_cdata()`](./xml/xmlwriter_start_cdata.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_comment()`](./xml/xmlwriter_start_comment.md) | `(mixed $writer): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_document()`](./xml/xmlwriter_start_document.md) | `(mixed $writer, ?string $version = '1.0', ?string $encoding = null, ?string $standalone = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd()`](./xml/xmlwriter_start_dtd.md) | `(mixed $writer, string $qualifiedName, ?string $publicId = null, ?string $systemId = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd_attlist()`](./xml/xmlwriter_start_dtd_attlist.md) | `(mixed $writer, string $name): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd_element()`](./xml/xmlwriter_start_dtd_element.md) | `(mixed $writer, string $qualifiedName): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_dtd_entity()`](./xml/xmlwriter_start_dtd_entity.md) | `(mixed $writer, string $name, bool $isParam): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_element()`](./xml/xmlwriter_start_element.md) | `(mixed $writer, string $name): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_element_ns()`](./xml/xmlwriter_start_element_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_start_pi()`](./xml/xmlwriter_start_pi.md) | `(mixed $writer, string $target): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_text()`](./xml/xmlwriter_text.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_attribute()`](./xml/xmlwriter_write_attribute.md) | `(mixed $writer, string $name, string $value): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_attribute_ns()`](./xml/xmlwriter_write_attribute_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace, string $value): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_cdata()`](./xml/xmlwriter_write_cdata.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_comment()`](./xml/xmlwriter_write_comment.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd()`](./xml/xmlwriter_write_dtd.md) | `(mixed $writer, string $name, ?string $publicId = null, ?string $systemId = null, ?string $content = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd_attlist()`](./xml/xmlwriter_write_dtd_attlist.md) | `(mixed $writer, string $name, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd_element()`](./xml/xmlwriter_write_dtd_element.md) | `(mixed $writer, string $name, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_dtd_entity()`](./xml/xmlwriter_write_dtd_entity.md) | `(mixed $writer, string $name, string $content, bool $isParam = false, ?string $publicId = null, ?string $systemId = null, ?string $notationData = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_element()`](./xml/xmlwriter_write_element.md) | `(mixed $writer, string $name, ?string $content = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_element_ns()`](./xml/xmlwriter_write_element_ns.md) | `(mixed $writer, ?string $prefix, string $name, ?string $namespace, ?string $content = null): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_pi()`](./xml/xmlwriter_write_pi.md) | `(mixed $writer, string $target, string $content): bool` | `bool` | ✓ | ✓ |
| [`xmlwriter_write_raw()`](./xml/xmlwriter_write_raw.md) | `(mixed $writer, string $content): bool` | `bool` | ✓ | ✓ |
