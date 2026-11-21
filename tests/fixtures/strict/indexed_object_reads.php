<?php
class IndexingProtocolOffsets implements ArrayAccess {
    public int $reads = 0;
    public int $probes = 0;
    public function offsetExists(mixed $offset): bool { $this->probes++; return false; }
    public function offsetGet(mixed $offset): mixed { $this->reads++; return 'provided'; }
    public function offsetSet(mixed $offset, mixed $value): void {}
    public function offsetUnset(mixed $offset): void {}
}
function indexingProtocolBox(mixed $object): mixed { return $object['anything']; }
$offsets = new IndexingProtocolOffsets();
echo $offsets['anything'], ':', $offsets->reads, ':', $offsets->probes, '|';
echo indexingProtocolBox($offsets), ':', $offsets->reads, ':', $offsets->probes, '|';
class IndexingProtocolNode { public array $rows = [1]; }
function indexingProtocolProbe(?IndexingProtocolNode $node): void {
    echo $node?->rows[9] ?? 'q';
}
indexingProtocolProbe(null);
indexingProtocolProbe(new IndexingProtocolNode());
