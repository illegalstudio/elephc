//! Purpose:
//! Implements PHP's Windows-only `imagegrabscreen()` and `imagegrabwindow()`
//! bridge entry points with native User32/GDI capture and owned RGBA storage.
//!
//! Called from:
//! - The target-aware image prelude through `elephc_img_grab_screen` and
//!   `elephc_img_grab_window`.
//!
//! Key details:
//! - Windows captures are converted from top-down BGRA DIB pixels to opaque
//!   RGBA before entering the shared image handle table.
//! - Every acquired DC, bitmap, and selected object is released on all paths.
//! - Non-Windows archives retain stable ABI stubs, although their PHP surface
//!   is omitted by the target-aware prelude.

#[cfg(windows)]
mod windows {
    use core::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null_mut;

    use image::RgbaImage;
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
        HGDIOBJ, SRCCOPY,
    };
    use windows_sys::Win32::Storage::Xps::PrintWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetDesktopWindow, GetWindowRect, IsWindow,
    };

    use crate::{ffi_guard, insert_image, ImageObj};

    const INVALID_WINDOW_NOTICE: &[u8] = b"Notice: imagegrabwindow(): Invalid window handle\n";

    extern "C" {
        fn __rt_diag_warning_c(message: *const u8, length: usize);
    }

    /// Sends the PHP notice through the generated runtime diagnostic channel.
    ///
    /// The bridge must not write stderr itself: the runtime adapter preserves
    /// `@` suppression and the target's diagnostic sink.
    fn emit_invalid_window_notice() {
        unsafe {
            __rt_diag_warning_c(INVALID_WINDOW_NOTICE.as_ptr(), INVALID_WINDOW_NOTICE.len());
        }
    }

    /// Owns the GDI objects acquired for one capture until conversion finishes.
    struct CaptureObjects {
        screen_dc: HDC,
        memory_dc: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
    }

    impl CaptureObjects {
        /// Restores the original selected object before `GetDIBits`, which
        /// requires the queried bitmap not to remain selected into a DC.
        unsafe fn deselect_bitmap(&mut self) {
            if !self.previous.is_null() {
                SelectObject(self.memory_dc, self.previous);
                self.previous = null_mut();
            }
        }
    }

    impl Drop for CaptureObjects {
        fn drop(&mut self) {
            unsafe {
                if !self.previous.is_null() {
                    SelectObject(self.memory_dc, self.previous);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
                if !self.memory_dc.is_null() {
                    DeleteDC(self.memory_dc);
                }
                if !self.screen_dc.is_null() {
                    ReleaseDC(null_mut(), self.screen_dc);
                }
            }
        }
    }

    /// Captures either the desktop or one HWND into a top-down RGBA image.
    unsafe fn capture(hwnd: HWND, client_area: bool, desktop: bool) -> Option<RgbaImage> {
        if hwnd.is_null() || (!desktop && IsWindow(hwnd) == 0) {
            return None;
        }

        let mut rect = RECT::default();
        let rect_ok = if client_area {
            GetClientRect(hwnd, &mut rect)
        } else {
            GetWindowRect(hwnd, &mut rect)
        };
        if rect_ok == 0 {
            return None;
        }

        let raw_width = rect.right.checked_sub(rect.left)?;
        let height = rect.bottom.checked_sub(rect.top)?;
        // php-src rounds captures down to a four-pixel boundary.
        let width = (raw_width / 4) * 4;
        if width <= 0 || height <= 0 {
            return None;
        }

        let screen_dc = GetDC(null_mut());
        if screen_dc.is_null() {
            return None;
        }
        let memory_dc = CreateCompatibleDC(screen_dc);
        if memory_dc.is_null() {
            ReleaseDC(null_mut(), screen_dc);
            return None;
        }
        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        if bitmap.is_null() {
            DeleteDC(memory_dc);
            ReleaseDC(null_mut(), screen_dc);
            return None;
        }
        let previous = SelectObject(memory_dc, bitmap);
        if previous.is_null() || previous as isize == -1 {
            DeleteObject(bitmap);
            DeleteDC(memory_dc);
            ReleaseDC(null_mut(), screen_dc);
            return None;
        }
        let mut objects = CaptureObjects {
            screen_dc,
            memory_dc,
            bitmap,
            previous,
        };

        if desktop {
            let _ = BitBlt(
                memory_dc,
                0,
                0,
                width,
                height,
                screen_dc,
                rect.left,
                rect.top,
                SRCCOPY,
            );
        } else {
            let _ = PrintWindow(hwnd, memory_dc, u32::from(client_area));
        }

        objects.deselect_bitmap();
        let pixel_count = (width as usize).checked_mul(height as usize)?;
        let byte_count = pixel_count.checked_mul(4)?;
        let mut pixels = vec![0_u8; byte_count];
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let scanlines = GetDIBits(
            memory_dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr().cast::<c_void>(),
            &mut info,
            DIB_RGB_COLORS,
        );
        if scanlines != height {
            return None;
        }
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        RgbaImage::from_raw(width as u32, height as u32, pixels)
    }

    /// Captures the complete Windows desktop and returns an image-table handle.
    #[no_mangle]
    pub extern "C" fn elephc_img_grab_screen() -> i64 {
        ffi_guard(-1, || unsafe {
            capture(GetDesktopWindow(), false, true)
                .map(|image| insert_image(ImageObj::new(image, true)))
                .unwrap_or(-1)
        })
    }

    /// Captures one HWND (whole window or client area) into the image table.
    #[no_mangle]
    pub extern "C" fn elephc_img_grab_window_status(handle: i64, client_area: i64) -> i64 {
        ffi_guard(-1, move || unsafe {
            let hwnd = handle as isize as HWND;
            if hwnd.is_null() || IsWindow(hwnd) == 0 {
                emit_invalid_window_notice();
                return -2;
            }
            capture(hwnd, client_area != 0, false)
                .map(|image| insert_image(ImageObj::new(image, true)))
                .unwrap_or(-1)
        })
    }

    /// Legacy bridge entry retained for callers compiled before the status
    /// distinction was introduced; invalid HWNDs still collapse to `false`.
    #[no_mangle]
    pub extern "C" fn elephc_img_grab_window(handle: i64, client_area: i64) -> i64 {
        let status = elephc_img_grab_window_status(handle, client_area);
        if status == -2 { -1 } else { status }
    }
}

/// Stable non-Windows ABI stub; the target-aware PHP prelude omits this symbol.
#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn elephc_img_grab_screen() -> i64 {
    -1
}

/// Stable non-Windows ABI stub; the target-aware PHP prelude omits this symbol.
#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn elephc_img_grab_window(_handle: i64, _client_area: i64) -> i64 {
    -1
}

/// Stable non-Windows ABI stub; the target-aware PHP prelude omits this symbol.
#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn elephc_img_grab_window_status(_handle: i64, _client_area: i64) -> i64 {
    -1
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Host-side ABI regression tests for Windows-only image capture fallbacks.
    //!
    //! Called from:
    //! - `cargo test -p elephc-image`.
    //!
    //! Key details:
    //! - Non-Windows builds must fail closed because PHP does not expose these
    //!   functions outside Windows.

    #[cfg(not(windows))]
    #[test]
    fn non_windows_capture_stubs_fail_closed() {
        assert_eq!(super::elephc_img_grab_screen(), -1);
        assert_eq!(super::elephc_img_grab_window(0, 0), -1);
        assert_eq!(super::elephc_img_grab_window_status(0, 0), -1);
    }
}
