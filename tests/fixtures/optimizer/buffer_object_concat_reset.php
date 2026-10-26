<?php
final class FormattingSlots {
    public buffer<int> $slots;
    public function __construct() { $this->slots = buffer_new<int>(4); }
    public function at(int $index): int { return $this->slots[$index & 3]; }
}
function showRows(FormattingSlots $table): void {
    $a = 1;
    $b = 2;
    // Two iterations, with scalar-only header updates and no separate update-statement reset.
    for (; (($a = $a ^ $b) & ($b = $b ^ $a)) > 0;) {
        echo sprintf("%40000d", $table->at(0));
    }
}
$table = new FormattingSlots();
showRows($table);
echo "\ndone\n";
$slots = $table->slots;
buffer_free($slots);
