<?php

// HTML Tidy repairs malformed markup and reports what it changed.
$html = <<<HTML
<html>
<head><title>Broken</title>
<body>
<p>Hello <b>world</i>
</body>
</html>
HTML;

$tidy = tidy_parse_string($html, ['indent' => true, 'wrap' => 80]);
tidy_clean_repair($tidy);

echo tidy_get_output($tidy);

$errors = tidy_get_error_buffer($tidy);
if ($errors !== false) {
    fwrite(STDERR, $errors);
}
