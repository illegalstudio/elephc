<?php
// WeakMap: an object-keyed map. Each key is an object instance (identity, not value).
// elephc implements WeakMap as a strong map: get/set/count/isset/unset and iteration
// behave like PHP while the key object is live. Auto-eviction when a key object is
// garbage-collected is not implemented (documented gap) — see docs/php/spl.md.

class Node {
    public int $id;
    public function __construct(int $id) {
        $this->id = $id;
    }
}

$cache = new WeakMap();

$a = new Node(1);
$b = new Node(2);

// Store values keyed by object identity.
$cache[$a] = "first";
$cache[$b] = "second";

echo count($cache);
echo "\n";

// Read back via ArrayAccess.
echo $cache[$a];
echo ":";
echo $cache[$b];
echo "\n";

// Update an existing key.
$cache[$a] = "FIRST";
echo $cache[$a];
echo "\n";

// isset on present and absent keys.
echo isset($cache[$a]) ? "yes" : "no";
echo ":";
$c = new Node(3);
echo isset($cache[$c]) ? "yes" : "no";
echo "\n";

// Iterate: foreach yields the object key and its mapped value.
foreach ($cache as $key => $value) {
    echo $key->id;
    echo "=";
    echo $value;
    echo ";";
}
echo "\n";

// Remove a single entry.
unset($cache[$b]);
echo count($cache);
echo "\n";