//! Purpose:
//! End-to-end fixtures for `ext/xmlwriter`: the object and procedural APIs, indentation,
//! namespaces, DTDs, escaping, PHP's argument validation errors, `outputMemory()` /
//! `flush()` return shapes and URI output.
//!
//! Called from:
//! - `cargo test --test codegen_tests xml::writer` through Rust's test harness.
//!
//! Key details:
//! - Expected bytes are PHP 8.5.10 / libxml2 2.15.3 output.

use crate::support::*;

/// A document written through the object API is byte-identical to PHP's.
#[test]
fn test_xmlwriter_object_api_document() {
    if skip_without_xml_native("test_xmlwriter_object_api_document") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$w = new XMLWriter();
var_dump($w->openMemory(), get_class($w), $w instanceof XMLWriter);
$w->startDocument('1.0', 'UTF-8');
$w->startElement('root');
$w->writeAttribute('a', 'v<&>"\'');
$w->startElement('child');
$w->text('t<&>"\'');
$w->endElement();
$w->writeElement('e', 'c');
$w->writeElement('empty');
$w->writeElement('emptystr', '');
$w->startElement('x'); $w->endElement();
$w->startElement('y'); $w->fullEndElement();
$w->writeComment('cm');
$w->writePi('pi', 'data');
$w->writeCdata('cd]]>x');
$w->writeRaw('<raw/>');
$w->endElement();
$w->endDocument();
echo $w->outputMemory(), "|END\n";
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nstring(9) \"XMLWriter\"\nbool(true)\n<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root a=\"v&lt;&amp;&gt;&quot;'\"><child>t&lt;&amp;&gt;&quot;'</child><e>c</e><empty/><emptystr></emptystr><x/><y></y><!--cm--><?pi data?><![CDATA[cd]]>x]]><raw/></root>\n|END\n"
    );
}

/// Indentation follows libxml2's rules for elements, text, comments, PIs and CDATA, and
/// the procedural API drives the same writer.
#[test]
fn test_xmlwriter_procedural_indentation() {
    if skip_without_xml_native("test_xmlwriter_procedural_indentation") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$w = xmlwriter_open_memory();
xmlwriter_set_indent($w, true);
xmlwriter_set_indent_string($w, "\t");
xmlwriter_start_document($w);
xmlwriter_start_element($w, 'root');
xmlwriter_start_element($w, 'a');
xmlwriter_text($w, 'txt');
xmlwriter_end_element($w);
xmlwriter_start_element($w, 'b');
xmlwriter_start_element($w, 'c');
xmlwriter_end_element($w);
xmlwriter_end_element($w);
xmlwriter_write_element($w, 'd', 'x');
xmlwriter_start_element($w, 'e');
xmlwriter_write_comment($w, 'cm');
xmlwriter_write_pi($w, 'p', 'd');
xmlwriter_write_cdata($w, 'cd');
xmlwriter_end_element($w);
xmlwriter_start_element($w, 'f'); xmlwriter_write_raw($w, 'r'); xmlwriter_end_element($w);
xmlwriter_start_element($w, 'g'); xmlwriter_text($w, ''); xmlwriter_end_element($w);
xmlwriter_start_element($w, 'h'); xmlwriter_full_end_element($w);
xmlwriter_end_element($w);
xmlwriter_end_document($w);
echo xmlwriter_output_memory($w), "|END\n";
$w = \XMLWRITER_OPEN_MEMORY(); $w->setIndent(true);
$w->startElement('a'); $w->startElement('b'); $w->text('t'); $w->startElement('c'); $w->endElement(); $w->endElement(); $w->endElement();
echo $w->outputMemory(), "|END\n";
"#,
    );
    assert_eq!(
        out,
        "<?xml version=\"1.0\"?>\n<root>\n\t<a>txt</a>\n\t<b>\n\t\t<c/>\n\t</b>\n\t<d>x</d>\n\t<e>\n\t\t<!--cm-->\n<?p d?>\n<![CDATA[cd]]></e>\n\t<f>r</f>\n\t<g></g>\n\t<h></h>\n</root>\n|END\n<a>\n <b>t  <c/>\n </b>\n</a>\n|END\n"
    );
}

/// Namespaced elements/attributes and DTD sections reproduce PHP's declarations layout.
#[test]
fn test_xmlwriter_namespaces_and_dtd() {
    if skip_without_xml_native("test_xmlwriter_namespaces_and_dtd") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$w = new XMLWriter(); $w->openMemory(); $w->setIndent(true);
$w->startElementNs('p', 'root', 'urn:p');
$w->writeAttributeNs('p', 'a', 'urn:p', 'v');
$w->writeAttributeNs(null, 'b', 'urn:b', 'v2');
$w->startAttributeNs('q', 'c', 'urn:q'); $w->text('v3'); $w->endAttribute();
$w->startElementNs(null, 'child', 'urn:c'); $w->endElement();
$w->startElementNs('p', 'child2', null); $w->endElement();
$w->writeElementNs('p', 'e', 'urn:p', 'content');
$w->writeElementNs(null, 'e2', null, null);
$w->endElement();
echo $w->outputMemory(), "|END\n";
$w = new XMLWriter(); $w->openMemory(); $w->setIndent(true);
$w->startDocument('1.0', 'UTF-8', 'yes');
$w->startDtd('html', '-//W3C//DTD XHTML 1.0//EN', 'http://www.w3.org/TR/xhtml1/DTD/xhtml1.dtd');
$w->startDtdElement('html'); $w->text('(head, body)'); $w->endDtdElement();
$w->writeDtdElement('head', '(title)');
$w->startDtdAttlist('a'); $w->text('href CDATA #REQUIRED'); $w->endDtdAttlist();
$w->writeDtdAttlist('img', 'src CDATA #REQUIRED');
$w->startDtdEntity('e1', false); $w->text('v1'); $w->endDtdEntity();
$w->writeDtdEntity('e2', 'v2');
$w->writeDtdEntity('e3', 'v3', true);
$w->writeDtdEntity('e4', '', false, 'pub', 'sys');
$w->writeDtdEntity('e5', '', false, null, 'sys5', 'ndata');
$w->endDtd();
$w->startElement('html'); $w->endElement();
$w->endDocument();
echo $w->outputMemory(), "|END\n";
$w = new XMLWriter(); $w->openMemory();
$w->writeDtd('b', null, 'sys'); $w->writeDtd('c', 'pub', 'sys', '<!ELEMENT c ANY>');
echo $w->outputMemory(), "|END\n";
"#,
    );
    assert_eq!(
        out,
        "<p:root p:a=\"v\" b=\"v2\" q:c=\"v3\" xmlns:q=\"urn:q\" xmlns=\"urn:b\" xmlns:p=\"urn:p\">\n <child xmlns=\"urn:c\"/>\n <p:child2/>\n <p:e xmlns:p=\"urn:p\">content</p:e>\n <e2/>\n</p:root>\n|END\n<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<!DOCTYPE html\nPUBLIC \"-//W3C//DTD XHTML 1.0//EN\"\n       \"http://www.w3.org/TR/xhtml1/DTD/xhtml1.dtd\" [\n <!ELEMENT html (head, body)>\n <!ELEMENT head (title)>\n <!ATTLIST a href CDATA #REQUIRED>\n <!ATTLIST img src CDATA #REQUIRED>\n <!ENTITY e1 \"v1\">\n <!ENTITY e2 \"v2\">\n <!ENTITY % e3 \"v3\">\n <!ENTITY e4 PUBLIC \"pub\" \"sys\">\n <!ENTITY e5 SYSTEM \"sys5\" NDATA ndata>\n]>\n<html/>\n|END\n<!DOCTYPE b SYSTEM \"sys\"><!DOCTYPE c PUBLIC \"pub\" \"sys\" [<!ELEMENT c ANY>]>|END\n"
    );
}

/// State errors answer false (and, like libxml2, `endDocument()` on an empty writer leaves
/// it in a state where later text is dropped), name validation raises PHP's `ValueError`s
/// with their exact argument labels, and an unopened writer raises PHP's `Error`.
#[test]
fn test_xmlwriter_validation_and_state_errors() {
    if skip_without_xml_native("test_xmlwriter_validation_and_state_errors") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$w = new XMLWriter(); $w->openMemory();
var_dump($w->endElement(), $w->endAttribute(), $w->endDocument());
$w->startElement('a'); $w->text('t'); var_dump($w->writeAttribute('x', '1'), $w->startAttribute('y')); $w->endElement();
echo $w->outputMemory(), "|END\n";
foreach (['', 'a b', '1a'] as $n) { try { $w->startElement($n); } catch (ValueError $e) { echo $e->getMessage(), "\n"; } }
try { xmlwriter_start_element($w, ''); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
$w->startElement('a');
try { $w->writeAttribute('', 'v'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { xmlwriter_write_attribute($w, 'a b', 'v'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { $w->startPi(''); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { $w->writeElementNs('p', 'a b', 'u', 'c'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { $w->writeDtdElement('', 'ANY'); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { $w->startDtdEntity('a b', false); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
var_dump($w->writePi('xml', 'x'), $w->startElement('a:b'));
$u = new XMLWriter();
try { $u->startElement('a'); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xmlwriter_text($u, 'x'); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
try { xmlwriter_open_uri(""); } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nbool(false)\nbool(true)\nbool(true)\nbool(true)\n\n<a x=\"1\" y=\"\"|END\nXMLWriter::startElement(): Argument #2 must be a valid element name, \"\" given\nXMLWriter::startElement(): Argument #2 must be a valid element name, \"a b\" given\nXMLWriter::startElement(): Argument #2 must be a valid element name, \"1a\" given\nxmlwriter_start_element(): Argument #2 ($name) must be a valid element name, \"\" given\nXMLWriter::writeAttribute(): Argument #2 ($value) must be a valid attribute name, \"\" given\nxmlwriter_write_attribute(): Argument #2 ($name) must be a valid attribute name, \"a b\" given\nXMLWriter::startPi(): Argument #2 must be a valid PI target, \"\" given\nXMLWriter::writeElementNs(): Argument #3 ($namespace) must be a valid element name, \"a b\" given\nXMLWriter::writeDtdElement(): Argument #2 ($content) must be a valid element name, \"\" given\nXMLWriter::startDtdEntity(): Argument #2 ($isParam) must be a valid attribute name, \"a b\" given\nbool(false)\nbool(false)\nError: Invalid or uninitialized XMLWriter object\nError: Invalid or uninitialized XMLWriter object\nValueError: xmlwriter_open_uri(): Argument #1 ($uri) must not be empty\n"
    );
}

/// `outputMemory()` / `flush()` return the buffered string for memory writers; URI writers
/// buffer until `flush()`, `endDocument()` or destruction and answer byte counts.
#[test]
fn test_xmlwriter_flush_semantics_and_uri_output() {
    if skip_without_xml_native("test_xmlwriter_flush_semantics_and_uri_output") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
$w = new XMLWriter(); $w->openMemory();
$w->startElement('a'); $w->text('1');
var_dump($w->outputMemory(false), $w->outputMemory(false), $w->outputMemory(), $w->outputMemory());
$w->text('2'); $w->endElement(); var_dump($w->outputMemory(true), $w->flush(), $w->flush(false));
$w = new XMLWriter(); $w->openMemory(); $w->startElement('a'); var_dump($w->flush(false), $w->flush(true), $w->flush());
$path = "elephc_xw_fixture.xml";
$w = new XMLWriter(); var_dump($w->openUri($path)); $w->setIndent(true); $w->startDocument(); $w->startElement('a'); $w->text('x');
var_dump(file_get_contents($path), $w->flush(), file_get_contents($path), $w->flush(false));
$w->endElement(); $w->endDocument(); var_dump($w->outputMemory(), $w->flush(), file_get_contents($path));
unset($w);
$w = new XMLWriter(); $w->openUri($path); $w->startElement('b'); $w->text('y'); unset($w); var_dump(file_get_contents($path));
unlink($path);
$w = XMLWriter::toUri("php://output"); $w->writeElement('u'); var_dump($w->flush());
$w = XMLWriter::toMemory(); $w->writeElement('m'); echo $w->outputMemory(), "\n";
try { @xmlwriter_open_uri("/nonexistent-elephc-dir/x.xml"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
class MyWriter extends XMLWriter { function hello(): string { return "hi"; } }
$m = new MyWriter(); $m->openMemory(); $m->writeElement('sub'); echo $m->hello(), " ", get_class($m), " ", $m->outputMemory(), "\n";
"#,
    );
    assert_eq!(
        out,
        "string(4) \"<a>1\"\nstring(4) \"<a>1\"\nstring(4) \"<a>1\"\nstring(0) \"\"\nstring(5) \"2</a>\"\nstring(0) \"\"\nstring(0) \"\"\nstring(2) \"<a\"\nstring(2) \"<a\"\nstring(0) \"\"\nbool(true)\nstring(0) \"\"\nint(26)\nstring(26) \"<?xml version=\"1.0\"?>\n<a>x\"\nint(0)\nstring(0) \"\"\nint(0)\nstring(31) \"<?xml version=\"1.0\"?>\n<a>x</a>\n\"\nstring(4) \"<b>y\"\n<u/>int(4)\n<m/>\nxmlwriter_open_uri(): Argument #1 ($uri) must resolve to a valid file path\nhi MyWriter <sub/>\n"
    );
}

/// Multi-byte output encodings carry NUL bytes, which the bridge hands over binary-safe
/// (hex-encoded across the C ABI): `outputMemory()` in both modes, `flush()` and a URI
/// writer all deliver PHP's exact bytes.
#[test]
fn test_xmlwriter_multibyte_output_encodings() {
    if skip_without_xml_native("test_xmlwriter_multibyte_output_encodings") {
        return;
    }
    let out = compile_and_run(
        r#"<?php
foreach (["UTF-16", "UTF-16LE", "UTF-16BE", "UCS-4", "ISO-8859-1"] as $enc) {
    $w = new XMLWriter(); $w->openMemory(); $w->startDocument("1.0", $enc); $w->writeElement("r", "é");
    $s = $w->outputMemory(false); $t = $w->outputMemory(true);
    echo $enc, ": ", strlen($s), " ", bin2hex(substr($s, 0, 12)), " ", $s === $t ? "same" : "differ", " ", strlen($w->outputMemory()), "\n";
}
$path = "elephc_xw_utf16.xml";
$w = new XMLWriter(); $w->openUri($path); $w->startDocument("1.0", "UTF-16LE"); $w->writeElement("r", "é"); $w->endDocument(); $w->flush();
unset($w);
echo "file: ", strlen(file_get_contents($path)), " ", bin2hex(substr(file_get_contents($path), 0, 8)), "\n";
unlink($path);
"#,
    );
    assert_eq!(out, "UTF-16: 98 fffe3c003f0078006d006c00 same 0\nUTF-16LE: 100 3c003f0078006d006c002000 same 0\nUTF-16BE: 100 003c003f0078006d006c0020 same 0\nUCS-4: 188 0000003c0000003f00000078 same 0\nISO-8859-1: 52 3c3f786d6c2076657273696f same 0\nfile: 102 3c003f0078006d00\n");
}
