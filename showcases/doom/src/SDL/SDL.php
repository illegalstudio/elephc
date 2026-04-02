<?php

namespace Showcases\Doom\SDL;

class SDL {
    public $window;
    public $renderer;

    public function __construct() {
        $this->window = ptr_null();
        $this->renderer = ptr_null();
    }

    public function boot($config): bool {
        if (SDL_Init(32) != 0) {
            return false;
        }

        $this->window = SDL_CreateWindow(
            $config->windowTitle,
            100,
            100,
            $config->windowWidth,
            $config->windowHeight,
            0
        );
        if (ptr_is_null($this->window)) {
            SDL_Quit();
            return false;
        }

        $this->renderer = SDL_CreateRenderer($this->window, -1, 2);
        if (ptr_is_null($this->renderer)) {
            SDL_DestroyWindow($this->window);
            $this->window = ptr_null();
            SDL_Quit();
            return false;
        }

        return true;
    }

    public function clear(int $r, int $g, int $b): void {
        SDL_SetRenderDrawColor($this->renderer, $r, $g, $b, 255);
        SDL_RenderClear($this->renderer);
    }

    public function present(): void {
        SDL_RenderPresent($this->renderer);
    }

    public function pumpEvents(): void {
        SDL_PumpEvents();
    }

    public function keyboardState(): ptr {
        return SDL_GetKeyboardState(ptr_null());
    }

    public function ticks(): int {
        return SDL_GetTicks();
    }

    public function delay(int $ms): void {
        SDL_Delay($ms);
    }

    public function lastError(): string {
        return SDL_GetError();
    }

    public function shutdown(): void {
        if (!ptr_is_null($this->renderer)) {
            SDL_DestroyRenderer($this->renderer);
            $this->renderer = ptr_null();
        }
        if (!ptr_is_null($this->window)) {
            SDL_DestroyWindow($this->window);
            $this->window = ptr_null();
        }
        SDL_Quit();
    }
}
