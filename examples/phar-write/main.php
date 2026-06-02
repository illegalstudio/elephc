<?php

// Writing a phar:// archive (Milestone 1: one uncompressed, signed entry).
//
// fopen("phar://<archive>/<entry>", "w") returns a write stream; fwrite() buffers
// the entry content, and fclose() assembles a native PHAR — PHP stub, a one-file
// manifest, the entry bytes, and a SHA1 signature trailer — and writes it to disk.
//
// The archive sets the PHAR_HDR_SIGNATURE flag and carries a real SHA1 signature,
// so the resulting .phar is accepted by real PHP (which requires a hash by
// default), e.g.:
//
//     php -r '$p = new Phar("hello.phar"); echo $p["greeting.txt"]->getContent();'
//
// Note: a LITERAL phar:// URL is read at compile time, so a literal read cannot
// see a phar this program wrote at run time. A NON-literal phar:// URL (a runtime
// concatenation) is read at run time instead — so the read-back below works.

$stream = fopen("phar://hello.phar/greeting.txt", "w");
$written = fwrite($stream, "Hello from an elephc-written phar!\n");
$ok = fclose($stream);

echo $ok ? "wrote hello.phar (" . $written . " bytes of content)\n"
         : "failed to write hello.phar\n";

// file_put_contents() is the one-call equivalent and writes the same signed archive.
$n = file_put_contents("phar://note.phar/readme.txt", "single-call phar write\n");
echo "wrote note.phar (" . $n . " bytes)\n";

// Read the archive back. Using a runtime (non-literal) path goes through the
// runtime phar reader, so a program can read a phar it just wrote in the same run.
$archive = "hello.phar";
$in = fopen("phar://" . $archive . "/greeting.txt", "r");
echo "read back: " . fread($in, 100);
fclose($in);

// file_get_contents() on a non-literal phar:// URL takes the same runtime path —
// it slurps the whole entry in one call.
echo "via file_get_contents: " . file_get_contents("phar://" . $archive . "/greeting.txt");
