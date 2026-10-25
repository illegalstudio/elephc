<?php

namespace Showcases\Doom\SDL;

use Showcases\Doom\App\Config;
use Showcases\Doom\Render\Screen;

final class SDL {
    public ptr $window;
    public ptr $renderer;
    public ptr $texture;
    public bool $vsync;

    public function __construct() {
        $this->window = ptr_null();
        $this->renderer = ptr_null();
        $this->texture = ptr_null();
        $this->vsync = false;
    }

    public function boot(Config $config): bool {
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

        // accelerated + vsync first, then whatever renderer the platform offers
        $this->renderer = SDL_CreateRenderer($this->window, -1, 6);
        if (ptr_is_null($this->renderer)) {
            $this->renderer = SDL_CreateRenderer($this->window, -1, 0);
        }
        if (ptr_is_null($this->renderer)) {
            $this->shutdown();
            return false;
        }

        // one streaming ARGB8888 texture at DOOM's resolution, stretched over the window
        $this->texture = SDL_CreateTexture($this->renderer, 372645892, 1, $config->renderWidth, $config->renderHeight);
        if (ptr_is_null($this->texture)) {
            $this->shutdown();
            return false;
        }

        $this->setVsync($config->vsync);
        return true;
    }

    public function setVsync(bool $enabled): void {
        int $flag = 0;
        if ($enabled) {
            $flag = 1;
        }
        SDL_RenderSetVSync($this->renderer, $flag);
        $this->vsync = $enabled;
    }

    public function present(Screen $screen): void {
        int $pitch = $screen->width * 4;
        SDL_UpdateTexture($this->texture, ptr_null(), buffer_ptr($screen->rgb), $pitch);
        SDL_RenderCopy($this->renderer, $this->texture, ptr_null(), ptr_null());
        SDL_RenderPresent($this->renderer);
    }

    public function setTitle(string $title): void {
        SDL_SetWindowTitle($this->window, $title);
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
        if (!ptr_is_null($this->texture)) {
            SDL_DestroyTexture($this->texture);
            $this->texture = ptr_null();
        }
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
