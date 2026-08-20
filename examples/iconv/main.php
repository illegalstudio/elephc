<?php
// The iconv extension converts text between character encodings and provides
// character-oriented string functions that count characters instead of bytes.

// 1. Transcoding. iconv() converts a byte string from one charset to another.
$utf8 = "Prüfung café";
$latin1 = iconv("UTF-8", "ISO-8859-1", $utf8);
echo "utf-8   : ", $utf8, "\n";
echo "latin-1 : ", bin2hex($latin1), "\n";
echo "back    : ", iconv("ISO-8859-1", "UTF-8", $latin1), "\n\n";

// The target charset accepts libc's suffixes: //TRANSLIT approximates characters
// the target cannot represent, and //IGNORE drops them. The approximation is the
// platform iconv's own, so glibc prints "cafe" where GNU libiconv prints "caf'e".
echo "translit: ", iconv("UTF-8", "ASCII//TRANSLIT", $utf8), "\n";
echo "ignore  : ", iconv("UTF-8", "ASCII//IGNORE", $utf8), "\n\n";

// 2. Character-oriented strings. strlen() counts bytes; iconv_strlen() counts
// characters, so multibyte text measures and slices the way a reader expects.
echo "bytes      : ", strlen($utf8), "\n";
echo "characters : ", iconv_strlen($utf8), "\n";
echo "substr     : ", iconv_substr($utf8, 0, 7), "\n";
echo "strpos     : ", iconv_strpos($utf8, "café"), "\n";
echo "strrpos    : ", iconv_strrpos($utf8, "f"), "\n\n";

// 3. Mail headers. RFC 2047 encoded-words carry non-ASCII text through headers
// that only allow ASCII. iconv_mime_encode() folds long fields automatically.
$subject = iconv_mime_encode("Subject", "Prüfung: Übersicht über alle Vorgänge");
echo $subject, "\n\n";

// Decoding turns one header field back into readable text...
echo "decoded : ", iconv_mime_decode($subject), "\n";

// ...and a whole header block becomes an array. A field name that appears more
// than once collects its values into a list.
$headers = iconv_mime_decode_headers(
    "Subject: =?ISO-8859-1?Q?Pr=FCfung?=\r\n" .
    "To: alice@example.com\r\n" .
    "To: bob@example.com\r\n" .
    "\r\n" .
    "message body"
);
echo "subject : ", $headers["Subject"], "\n";
echo "to      : ", implode(", ", $headers["To"]), "\n\n";

// 4. Default encodings. The character-oriented functions fall back to the
// internal encoding, which starts at UTF-8 and can be changed at runtime.
$encodings = iconv_get_encoding();
echo "internal: ", $encodings["internal_encoding"], "\n";
iconv_set_encoding("internal_encoding", "ISO-8859-1");
echo "latin-1 length of the same bytes: ", iconv_strlen($utf8), "\n";
iconv_set_encoding("internal_encoding", "UTF-8");
echo "utf-8 length of the same bytes  : ", iconv_strlen($utf8), "\n";
