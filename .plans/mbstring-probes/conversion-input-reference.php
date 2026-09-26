<?php
class ConversionReference {
    public ?Closure $change = null;
    public function __toString(): string {
        $change = $this->change;
        if ($change !== null) { $change(); }
        return "UTF-8";
    }
}
$changer = new ConversionReference();
$input = ["before"];
$slot =& $input[0];
$changer->change = function () use (&$slot): void { $slot = "after"; };
echo json_encode(mb_convert_encoding($input, "UTF-8", [$changer])), "\n";
