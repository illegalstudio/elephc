<?php
// C structs through FFI: `extern class` fields use the C widths and the natural C layout,
// so a struct declared with the header's field types has the header's offsets and sizeof.

// libc's `struct tm`: nine C ints, then a `long` and a `const char*`.
extern class Tm {
    public i32 $tm_sec;
    public i32 $tm_min;
    public i32 $tm_hour;
    public i32 $tm_mday;
    public i32 $tm_mon;
    public i32 $tm_year;
    public i32 $tm_wday;
    public i32 $tm_yday;
    public i32 $tm_isdst;
    public int $tm_gmtoff;   // C long
    public string $tm_zone;  // const char*
}

// SDL3-style structs: an int rectangle, a byte color, and a keyboard event.
extern class Rect { public i32 $x; public i32 $y; public i32 $w; public i32 $h; }
extern class Color { public u8 $r; public u8 $g; public u8 $b; public u8 $a; }
extern class KeyboardEvent {
    public u32 $type;
    public u32 $reserved;
    public u64 $timestamp;
    public u32 $windowID;
    public u32 $which;
    public u32 $scancode;
    public u32 $key;
    public u16 $mod;
    public u16 $raw;
    public bool $down;
    public bool $repeat;
}

extern "System" {
    function malloc(u64 $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, i32 $byte, u64 $count): ptr;
    function gmtime_r(ptr $timep, ptr<Tm> $result): ptr<Tm>;
}

// sizeof matches C: 9 * 4 bytes, 4 bytes of padding, then two 8-byte fields.
echo "sizeof(struct tm) = " . ptr_sizeof("Tm") . "\n";            // 56

$clock = malloc(8);
ptr_set($clock, 1700000000);                                      // a time_t
$tm = gmtime_r($clock, ptr_cast<Tm>(malloc(ptr_sizeof("Tm"))));
echo "date = " . ($tm->tm_year + 1900) . "-" . ($tm->tm_mon + 1) . "-" . $tm->tm_mday . "\n";
echo "time = " . $tm->tm_hour . ":" . $tm->tm_min . ":" . $tm->tm_sec . "\n";
echo "zone = " . $tm->tm_zone . "\n";

$rect = ptr_cast<Rect>(malloc(ptr_sizeof("Rect")));               // 16 bytes
$rect->x = 10;
$rect->y = 20;
$rect->w = 640;
$rect->h = intdiv($rect->w * 3, 4);                               // stored as a C int
echo "rect = " . $rect->x . "," . $rect->y . " " . $rect->w . "x" . $rect->h . "\n";

$color = ptr_cast<Color>(malloc(ptr_sizeof("Color")));            // 4 bytes
$color->r = 255;
$color->g = 300;                                                  // keeps the low byte, like C: 44
$color->b = 0;
$color->a = 128;
echo "color = " . $color->r . " " . $color->g . " " . $color->b . " " . $color->a . "\n";
echo "color as one u32 = " . ptr_read32($color) . "\n";

$event = ptr_cast<KeyboardEvent>(malloc(ptr_sizeof("KeyboardEvent")));
memset($event, 0, ptr_sizeof("KeyboardEvent"));
echo "sizeof(SDL_KeyboardEvent) = " . ptr_sizeof("KeyboardEvent") . "\n"; // 40
$event->type = 0x300;
$event->key = 97;
$event->mod = 0x8001;
$event->down = true;
echo "event type = " . $event->type . ", key = " . $event->key . ", mod = " . $event->mod
    . ", down = " . ($event->down ? "yes" : "no") . ", repeat = " . ($event->repeat ? "yes" : "no") . "\n";

free($event);
free($color);
free($rect);
free($tm);
free($clock);
