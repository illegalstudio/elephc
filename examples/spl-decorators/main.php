<?php
echo "iterator iterator:\n";
$wrapped = new IteratorIterator(new ArrayIterator(["one" => 1, "two" => 2]));
foreach ($wrapped as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "\n";
}

echo "limit over infinite:\n";
$cycle = new LimitIterator(new InfiniteIterator(new ArrayIterator(["A", "B"])), 0, 5);
foreach ($cycle as $value) {
    echo $value;
}
echo "\n";

echo "no rewind:\n";
$source = new ArrayIterator([10, 20, 30]);
$source->next();
$noRewind = new NoRewindIterator($source);
foreach ($noRewind as $value) {
    echo $value;
    echo " ";
}
echo "\n";

echo "seek:\n";
$limited = new LimitIterator(new ArrayIterator(["zero", "one", "two", "three"]), 1, 2);
$limited->seek(2);
echo $limited->key();
echo "=";
echo $limited->current();
echo "\n";
