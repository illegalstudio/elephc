<?php
echo "iterator iterator:\n";
$wrapped = new IteratorIterator(new ArrayObject(["one" => 1, "two" => 2]), "ArrayObject");
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

echo "filter:\n";
$minimum = 1;
$marker = "keep";

$filter = new CallbackFilterIterator(
    new ArrayIterator(["skip" => 1, "keep" => 2, "tail" => 3]),
    function (int $value, string $key, Iterator $inner) use ($minimum, $marker): bool {
        return $inner instanceof Iterator && ($value > $minimum || $key === $marker);
    }
);
foreach ($filter as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "\n";
}

echo "regex:\n";
$regex = new RegexIterator(
    new ArrayIterator(["first" => "item-10", "second" => "skip", "third" => "task-7"]),
    "/([a-z]+)-([0-9]+)/",
    RegexIterator::GET_MATCH
);
foreach ($regex as $key => $matches) {
    echo $key;
    echo "=";
    echo $matches[1];
    echo ":";
    echo $matches[2];
    echo "\n";
}

echo "cache:\n";
$cache = new CachingIterator(
    new ArrayIterator(["a" => "A", "b" => "B"]),
    CachingIterator::FULL_CACHE | CachingIterator::TOSTRING_USE_KEY
);
foreach ($cache as $key => $value) {
    echo (string) $cache;
    echo "=";
    echo $value;
    echo "/";
    echo $cache->hasNext() ? "more" : "last";
    echo "\n";
}
echo $cache["a"];
echo "\n";

echo "append:\n";
$append = new AppendIterator();
$append->append(new ArrayIterator(["left" => "L"]));
$append->append(new ArrayIterator(["right" => "R"]));
foreach ($append as $key => $value) {
    echo $append->getIteratorIndex();
    echo ":";
    echo $key;
    echo "=";
    echo $value;
    echo "\n";
}

echo "multiple:\n";
$multi = new MultipleIterator(MultipleIterator::MIT_NEED_ANY | MultipleIterator::MIT_KEYS_ASSOC);
$multi->attachIterator(new ArrayIterator(["a" => 1, "b" => 2]), "left");
$multi->attachIterator(new ArrayIterator(["x" => 10]), "right");
foreach ($multi as $keys => $values) {
    echo $keys["left"];
    echo "/";
    echo is_null($keys["right"]) ? "null" : $keys["right"];
    echo "=";
    echo $values["left"];
    echo "/";
    echo is_null($values["right"]) ? "null" : $values["right"];
    echo "\n";
}

echo "recursive:\n";
$tree = new RecursiveIteratorIterator(
    new RecursiveArrayIterator([
        "root" => ["left" => 1, "right" => ["leaf" => 2]],
        "tail" => 3,
    ]),
    RecursiveIteratorIterator::SELF_FIRST
);
foreach ($tree as $key => $value) {
    echo $tree->getDepth();
    echo ":";
    echo $key;
    echo "=";
    echo gettype($value) === "array" ? "array" : $value;
    echo "\n";
}

echo "recursive filter:\n";
$min = 1;
$recursiveFilter = new RecursiveCallbackFilterIterator(
    new RecursiveArrayIterator(["group" => ["skip" => 1, "keep" => 2], "tail" => 3]),
    function (mixed $value, mixed $key, Iterator $inner) use ($min): bool {
        return $inner instanceof Iterator
            && (gettype($value) === "array" || $value > $min || $key === "always");
    }
);
foreach (new RecursiveIteratorIterator($recursiveFilter) as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo "\n";
}

echo "recursive regex:\n";
$recursiveRegex = new RecursiveRegexIterator(
    new RecursiveArrayIterator(["keep" => ["apple" => 1, "skip" => 2], "drop" => ["pear" => 3]]),
    "/keep|apple/",
    RecursiveRegexIterator::MATCH,
    RecursiveRegexIterator::USE_KEY
);
foreach (new RecursiveIteratorIterator($recursiveRegex, RecursiveIteratorIterator::SELF_FIRST) as $key => $value) {
    echo $key;
    echo "=";
    echo gettype($value) === "array" ? "array" : $value;
    echo "\n";
}
