<?php
// Software framebuffer demo using off-heap memory plus SDL2 rendering.
// Run with:
// elephc -l SDL2 -L /opt/homebrew/lib examples/sdl_framebuffer/main.php

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
    function SDL_CreateRenderer(ptr $window, int $index, int $flags): ptr;
    function SDL_DestroyRenderer(ptr $renderer): void;
    function SDL_SetRenderDrawColor(ptr $renderer, int $r, int $g, int $b, int $a): int;
    function SDL_RenderClear(ptr $renderer): int;
    function SDL_RenderDrawPoint(ptr $renderer, int $x, int $y): int;
    function SDL_RenderPresent(ptr $renderer): void;
    function SDL_Delay(int $ms): void;
    function SDL_GetError(): string;
}

extern "System" {
    function malloc(int $size): ptr;
    function free(ptr $p): void;
    function memset(ptr $dest, int $byte, int $count): ptr;
}

$SDL_INIT_VIDEO = 32;
$SDL_RENDERER_ACCELERATED = 2;
$w = 64;
$h = 64;
$scale = 6;

if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "SDL_Init failed: " . SDL_GetError() . "\n";
    exit(1);
}

$window = SDL_CreateWindow("framebuffer demo", 100, 100, $w * $scale, $h * $scale, 0);
$renderer = SDL_CreateRenderer($window, -1, $SDL_RENDERER_ACCELERATED);
$buffer = malloc($w * $h);

if (ptr_is_null($window) || ptr_is_null($renderer) || ptr_is_null($buffer)) {
    echo "setup failed\n";
    if (!ptr_is_null($buffer)) { free($buffer); }
    if (!ptr_is_null($renderer)) { SDL_DestroyRenderer($renderer); }
    if (!ptr_is_null($window)) { SDL_DestroyWindow($window); }
    SDL_Quit();
    exit(1);
}

memset($buffer, 0, $w * $h);

for ($y = 0; $y < $h; $y++) {
    for ($x = 0; $x < $w; $x++) {
        $on = (($x / 8) % 2) === (($y / 8) % 2);
        ptr_write8(ptr_offset($buffer, $y * $w + $x), $on ? 255 : 40);
    }
}

SDL_SetRenderDrawColor($renderer, 10, 10, 16, 255);
SDL_RenderClear($renderer);

for ($y = 0; $y < $h; $y++) {
    for ($x = 0; $x < $w; $x++) {
        $shade = ptr_read8(ptr_offset($buffer, $y * $w + $x));
        SDL_SetRenderDrawColor($renderer, $shade, 160, 255 - $shade, 255);
        for ($sy = 0; $sy < $scale; $sy++) {
            for ($sx = 0; $sx < $scale; $sx++) {
                SDL_RenderDrawPoint($renderer, $x * $scale + $sx, $y * $scale + $sy);
            }
        }
    }
}

SDL_RenderPresent($renderer);
echo "framebuffer ok\n";
SDL_Delay(1000);

free($buffer);
SDL_DestroyRenderer($renderer);
SDL_DestroyWindow($window);
SDL_Quit();
