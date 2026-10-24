<?php
// Hosting a PHP extension: ext/wordstat is an ordinary C extension, the kind
// you would build with phpize. Elephc builds it from source and its functions,
// constant and exception class become part of this program.
//
//   elephc extension install     # builds ext/wordstat (declared in elephc.toml)
//   elephc main.php && ./main

$text = <<<TEXT
The compiler compiles PHP. The compiled program hosts the extension,
and the extension counts the words the program hands it.
TEXT;

echo "wordstat ", WORDSTAT_VERSION, "\n";

$counts = wordstat_count($text);
foreach (["the", "program", "extension", "words"] as $word) {
    echo str_pad($word, 10), $counts[$word], "\n";
}

echo "top 3: ", implode(", ", wordstat_top($text)), "\n";
echo "top 1: ", implode(", ", wordstat_top($text, 1)), "\n";

try {
    wordstat_count("  ... !!! ");
} catch (WordstatException $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}

try {
    wordstat_top($text, 0);
} catch (ValueError $e) {
    echo $e->getMessage(), "\n";
}
