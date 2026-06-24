<?php

// `App\Normalizer` (in src/Normalizer.php) is resolved through composer.json PSR-4 autoload.
// It contains a lazy `require $file` with a runtime-computed path that elephc cannot resolve at
// compile time. Rather than failing the whole build, elephc degrades that dynamic include into a
// runtime-fatal stub: the class still compiles, and this program -- which only ever takes the
// fast path -- runs normally.

use App\Normalizer;

$samples = ['hello', 'world'];

foreach ($samples as $s) {
    echo Normalizer::label($s) . "\n";
}

// Note: calling Normalizer::loadTable('canonicalComposition') here would hit the degraded
// dynamic include and abort with a clear "could not be resolved at compile time" fatal on stderr.
echo "done\n";
