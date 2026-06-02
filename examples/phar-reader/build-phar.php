<?php
// Regenerates app.phar (run with: php -d phar.readonly=0 build-phar.php).
// The compiled example embeds the entries at compile time, so app.phar is only
// needed when (re)compiling examples/phar-reader/main.php, not when running it.
$out = __DIR__ . '/app.phar';
@unlink($out);
$p = new Phar($out);
$p['greeting.txt'] = "Hello from inside a PHAR!\n";
$p['data/info.txt'] = "This entry lives in a sub-path of the archive.\n";
// A larger, compressible entry stored gzip-compressed (raw DEFLATE) — elephc
// inflates it at compile time, so reading it through phar:// is transparent.
$p['notes.txt'] = str_repeat("compressed phar entries read transparently. ", 8);
$p->setStub("<?php __HALT_COMPILER();");
$p['notes.txt']->compress(Phar::GZ);
echo "wrote " . $out . " (" . filesize($out) . " bytes)\n";
