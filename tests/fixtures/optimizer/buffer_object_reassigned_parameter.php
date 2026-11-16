<?php
final class ReassignedSlots {
    private buffer<int> $slots;
    public function __construct() {
        $this->slots = buffer_new<int>(4);
        $this->slots[0] = 2;
        $this->slots[1] = 7;
        $this->slots[2] = 11;
        $this->slots[3] = 13;
    }
    public function at(int $index): int {
        if ($index > 1) { $index = 1; }
        return $this->slots[$index];
    }
    public function clear(): void {
        $slots = $this->slots;
        buffer_free($slots);
    }
}
function showReassignedRows(ReassignedSlots $table, int $count): void {
    for ($index = 0; $index < $count; $index++) { echo $table->at($index), ' '; }
}
$table = new ReassignedSlots();
showReassignedRows($table, $argc + 3);
echo "\n";
$table->clear();
