<?php
// Checks input ownership independently of the native query registration fixture.
class QueryTernaryOwner {
    public function __construct(public string $kind) {}
    public function __destruct() { echo "destroy:", $this->kind, "\n"; }
}
function query_ternary_closure(): callable {
    $owner = new QueryTernaryOwner("closure");
    return function() use ($owner): void {};
}
function query_ternary_probe(bool $closure): void {
    $root = ["key" => $closure ? query_ternary_closure() : new QueryTernaryOwner("object")];
    unset($root["key"]);
    echo "removed:", count($root), "\n";
}
query_ternary_probe(false);
query_ternary_probe(true);
echo "done\n";
