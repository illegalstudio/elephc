<?php
interface Root {}
class Label implements Root { public int $value = 1; }
class LockedLabel implements Root { public int $value = 2; }
class Holder { public Root $item; public function __construct(Root $item) { $this->item = $item; } }
class StaticHolder { public static Root $item; }
function clear_after_rewrite(Holder $h, Root $other) {
    if ($h->item instanceof Label) { $h->item = $other; unset($h->item->value); }
    return isset($h->item->value) ? "set" : "unset";
}
function clear_plain(Holder $h) {
    if ($h->item instanceof Label) { unset($h->item->value); }
    try { return isset($h->item->value) ? "set" : "unset"; } catch (Error $e) { return "err"; }
}
function clear_static() {
    if (StaticHolder::$item instanceof Label) { unset(StaticHolder::$item->value); }
    return isset(StaticHolder::$item->value) ? "set" : "unset";
}
echo clear_after_rewrite(new Holder(new Label()), new LockedLabel()), "|";
echo clear_after_rewrite(new Holder(new LockedLabel()), new Label()), "|";
echo clear_plain(new Holder(new Label())), clear_plain(new Holder(new LockedLabel())), "|";
StaticHolder::$item = new Label();
echo clear_static(), "\n";
