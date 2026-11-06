<?php
// Exercise catalog lookup and several codec consumers in the same native binary.
mb_substitute_character(63);
echo implode(',', mb_list_encodings()), "\n";
foreach (['UTF-8', 'ASCII', 'SJIS', 'EUC-JP', 'GB18030', 'ISO-2022-JP'] as $encoding) {
    $bytes = mb_convert_encoding("Aéあ😀", $encoding, 'UTF-8');
    echo $encoding, '|', base64_encode($bytes), '|', mb_strlen($bytes, $encoding), "\n";
}
