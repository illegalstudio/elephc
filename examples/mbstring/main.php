<?php
header("Content-Type: text/plain; charset=UTF-8");

// Product objects can provide their display text through PHP's Stringable protocol.
class ProductLabel {
    public function __construct(public string $text) {}
    public function __toString(): string { return $this->text; }
}

// Normalize imported product labels and keep character and byte limits separate.
mb_internal_encoding("UTF-8");
// Convert buffered output with the same request encodings used by individual string operations.
mb_http_output("UTF-8");
ob_start("mb_output_handler");
mb_language("Japanese");
mb_detect_order(["UTF-8", "SJIS-win", "ASCII"]);
echo "Input encoding: ", mb_internal_encoding(), "\n";
echo "Language: ", mb_language(), "\n";
$import_encodings = mb_detect_order();
if (is_array($import_encodings)) { echo "Import encoding order: ", implode(", ", $import_encodings), "\n"; }
echo "HTTP output encoding: ", mb_http_output(), "\n";
echo "HTTP input encodings: ", mb_http_input("L"), "\n";
echo "Mail charset: ", mb_get_info("mail_charset"), "\n";

// Decode a submitted product search into named fields and repeated tags.
mb_parse_str("label=Caf%C3%A9&tags[]=new&tags[]=imported", $search);
echo "Search label: ", $search["label"], "; first tag: ", $search["tags"][0], "\n";

// Keep regex configuration explicit when import text and search rules use different encodings.
mb_regex_encoding("UTF-8");
$previous_regex_options = mb_regex_set_options("ir");

// Extract product codes progressively while retaining named captures and byte positions.
mb_ereg_search_init("New arrivals: SKU-120, SKU-240", "(?<code>SKU-[0-9]+)");
$code_match = mb_ereg_search_regs();
while (is_array($code_match)) {
    echo "Product code: ", (string)$code_match["code"], " (ends at byte ", mb_ereg_search_getpos(), ")\n";
    $code_match = mb_ereg_search_regs();
}
echo "Regex encoding: ", mb_regex_encoding(), "; options: ", mb_regex_set_options(), "\n";
echo "Oniguruma version: ", MB_ONIGURUMA_VERSION, "\n";
echo "Catalog code matches: ", mb_ereg_match("[a-z]+-[0-9]+$", "SKU-204") ? "yes" : "no", "\n";
mb_regex_set_options($previous_regex_options);

// Extract a supplier identifier directly into a caller-owned capture array.
if (mb_ereg("(?<supplier>SKU)-(?<number>[0-9]+)", "New arrival: SKU-204", $supplier_code)) {
    echo "Supplier: ", $supplier_code["supplier"], "; item: ", $supplier_code["number"], "\n";
}
if (mb_eregi("(?<label>café)", "CAFÉ 東京", $label_match)) {
    echo "Original cafe label: ", $label_match["label"], "\n";
}

// Accept comma- or semicolon-separated tags from supplier catalog exports.
$tags = mb_split("[,;][[:space:]]*", "新着, Café; 東京");
if (is_array($tags)) { echo "Product tags: ", implode(" | ", $tags), "\n"; }

// Format supplier codes with named captures and normalize a label regardless of letter case.
$display_codes = mb_ereg_replace("(?<prefix>SKU)-(?<number>[0-9]+)", "\\k<prefix> \\k<number>", "SKU-120, SKU-240");
if (is_string($display_codes)) { echo "Display codes: ", $display_codes, "\n"; }
$normalized_label = mb_eregi_replace("café", "Café", "CAFÉ 東京");
if (is_string($normalized_label)) { echo "Normalized label: ", $normalized_label, "\n"; }

$imported = "　ﾊﾟﾝ 東京　";
echo "Detected import encoding: ", mb_detect_encoding($imported, ["UTF-8", "SJIS-win"], true), "\n";
$labels = ["supplier" => [$imported, "Café"]];
echo "Imported labels are valid UTF-8: ", mb_check_encoding($labels) ? "yes" : "no", "\n";
// Decode a supplier export before using the same UTF-8 label pipeline.
$legacy_label = "Caf" . chr(233);
echo "Decoded supplier label: ", mb_convert_encoding($legacy_label, "UTF-8", "ISO-8859-1"), "\n";
$label = mb_convert_kana(mb_trim($imported));
echo "Product: ", $label, "\n";
echo "Display width: ", mb_strwidth(new ProductLabel($label)), "\n";
// Compose optional preview parameters before passing them as one argument list.
$preview_options = [0, 4, mb_internal_encoding()];
echo "Preview: ", mb_substr($label, ...$preview_options), "\n";
echo "First complete characters within 8 bytes: ", mb_strcut($label, 0, 8), "\n";
echo "Padded label: [", mb_str_pad($label, 12), "]\n";
echo "Without supplier prefix: ", mb_ltrim("※※" . $label, "※"), "\n";
echo "Without trailing marker: ", mb_rtrim($label . "＊＊", "＊"), "\n";
// Show a visible replacement marker when imported text contains malformed bytes.
mb_substitute_character(65533);
echo "Repaired invalid input: ", mb_scrub(chr(255) . $label), "\n";

// Positions count characters. Failed searches return false, independently of position zero.
$history = "東京: Café; 東京: CAFÉ";
echo "Tokyo entries: ", mb_substr_count($history, "東京"), "\n";
echo "First Tokyo position: ", mb_strpos($history, "東京"), "\n";
echo "Last Tokyo position: ", mb_strrpos($history, "東京"), "\n";
echo "First cafe position: ", mb_stripos($history, "café"), "\n";
echo "Last cafe position: ", mb_strripos($history, "café"), "\n";
echo "First entry suffix: ", mb_strstr($history, "Café"), "\n";
echo "First cafe suffix ignoring case: ", mb_stristr($history, "café"), "\n";
echo "Last Tokyo entry: ", mb_strrchr($history, "東京"), "\n";
echo "Last cafe suffix ignoring case: ", mb_strrichr($history, "café"), "\n";

// Ordinals represent Unicode codepoints, independently of their UTF-8 byte length.
echo "First codepoint: ", mb_ord($label), "\n";
echo "Catalog badge: ", mb_chr(9733), "\n";

// Split labels into printable fields and inspect the charset used by an exporter.
echo "Available import encodings: ", count(mb_list_encodings()), "\n";
echo "Label fields:";
foreach (mb_str_split($label, 3) as $field) { echo " [", (string)$field, "]"; }
echo "\nExport charset: ", mb_preferred_mime_name("UTF-8"), "\n";
echo "Accepted aliases:";
foreach (mb_encoding_aliases("UTF-8") as $alias) { echo " ", (string)$alias; }
echo "\n";

// Keep an ASCII representation for text-only export formats, then restore the label.
$entity_map = [128, 0x10FFFF, 0, 0xFFFFFFFF];
$export_label = mb_encode_numericentity($label, $entity_map, hex: true);
echo "ASCII label: ", $export_label, "\n";
echo "Restored label: ", mb_decode_numericentity($export_label, $entity_map), "\n";

// Decode a mail subject into the same internal encoding used for catalog labels.
echo "Mail subject: ", mb_decode_mimeheader("=?UTF-8?Q?Nouveaut=C3=A9s_du_catalogue?="), "\n";

// Encode an outgoing product subject for mail transports that expect ASCII headers.
echo "Outgoing subject: ", mb_encode_mimeheader($label, "UTF-8", "B"), "\n";

// Expand catalog placeholders with a PHP callback while retaining UTF-8 text.
function replace_catalog_label(array $matches): string {
    return mb_strtoupper($matches[1], "UTF-8");
}
echo "Shelf label: ", mb_ereg_replace_callback("\\{([^}]+)\\}", "replace_catalog_label", "Nouveautés: {café}"), "\n";
ob_end_flush();
