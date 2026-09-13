//! Keep a taskbar-overlapping floatbar above the Windows taskbar.
//!
//! The taskbar is itself topmost. Activating it can reorder the topmost band
//! without clearing `WS_EX_TOPMOST` on the floatbar. While the floatbar is
//! shown we run a short visibility-scoped worker and reassert topmost only when
//! the bar is visible and overlaps a taskbar. Move/resize always reasserts in
//! `floatbar::handle_window_event` (not overlap-gated). The worker owns every
//! Explorer/taskbar enumeration; the Tauri main thread only receives a small
//! coalesced reassert message.

#[cfg(any(windows, test))]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(any(windows, test))]
fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

#[cfg(windows)]
mod platform {
    use super::{Rect, rects_overlap};
    use raw_window_handle::HasWindowHandle;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Duration;
    use tauri::Manager;

    use super::super::window::{self, FLOATBAR_LABEL};

    /// Interval while the floatbar is active. Matches the ~150ms recovery
    /// budget validated for the original hook-based fix.
    const POLL_INTERVAL: Duration = Duration::from_millis(120);

    static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
    static FLOATBAR_ACTIVE: AtomicBool = AtomicBool::new(false);
    static LOOP_RUNNING: AtomicBool = AtomicBool::new(false);
    static REASSERT_PENDING: AtomicBool = AtomicBool::new(false);

    static PRIMARY_CLASS: OnceLock<Vec<u16>> = OnceLock::new();
    static SECONDARY_CLASS: OnceLock<Vec<u16>> = OnceLock::new();

    pub fn install(app: &tauri::AppHandle) {
        let _ = APP_HANDLE.set(app.clone());
    }

    pub fn set_active(active: bool) {
        FLOATBAR_ACTIVE.store(active, Ordering::Release);
        if active {
            ensure_loop();
        }
    }

    fn ensure_loop() {
        if LOOP_RUNNING.swap(true, Ordering::AcqRel) {
            return;
        }
        let Some(app) = APP_HANDLE.get().cloned() else {
            LOOP_RUNNING.store(false, Ordering::Release);
            return;
        };

        let worker = thread::Builder::new()
            .name("codexbar-floatbar-zorder".to_string())
            .spawn(move || {
                while FLOATBAR_ACTIVE.load(Ordering::Acquire) {
                    thread::sleep(POLL_INTERVAL);
                    if !FLOATBAR_ACTIVE.load(Ordering::Acquire) {
                        break;
                    }
                    probe_and_request_reassert(&app);
                }
                LOOP_RUNNING.store(false, Ordering::Release);
                // If set_active(true) raced with loop exit, start again.
                if FLOATBAR_ACTIVE.load(Ordering::Acquire) {
                    ensure_loop();
                }
            });
        if worker.is_err() {
            LOOP_RUNNING.store(false, Ordering::Release);
        }
    }

    /// Probe visibility and taskbar geometry away from the Tauri main thread.
    /// `FindWindow*`/`GetWindowRect` can synchronously wait for Explorer; doing
    /// that from the UI thread was the remaining source of periodic freezes.
    /// Probe visibility and taskbar geometry away from the Tauri main thread.
    /// `FindWindow*`/`GetWindowRect` can synchronously wait for Explorer; doing
    /// that from the UI thread was the remaining source of periodic freezes.
    fn probe_and_request_reassert(app: &tauri::AppHandle) {
        let Some(floatbar) = app.get_webview_window(FLOATBAR_LABEL) else {
            // Webview gone without hide — stop the guard.
            FLOATBAR_ACTIVE.store(false, Ordering::Release);
            return;
        };
        if !floatbar.is_visible().unwrap_or(false) {
            return;
        }
        let Ok(handle) = floatbar.window_handle() else {
            return;
        };
        let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let float_hwnd = handle.hwnd.get();

        let mut floatbar_rect = Rect::default();
        if unsafe { GetWindowRect(float_hwnd, &mut floatbar_rect) } == 0 {
            return;
        }

        let taskbars = taskbars();
        let overlapping_taskbars: Vec<isize> = taskbars
            .into_iter()
            .filter(|(_, r)| rects_overlap(floatbar_rect, *r))
            .map(|(h, _)| h)
            .collect();

        if overlapping_taskbars.is_empty() {
            return;
        }

        // Only request reassert if an overlapping taskbar is actually ABOVE floatbar in z-order.
        // Prevents redundant SetWindowPos calls every 120ms that cause continuous DWM flickering.
        const GW_HWNDPREV: u32 = 3;
        let mut curr = unsafe { GetWindow(float_hwnd, GW_HWNDPREV) };
        let mut taskbar_above = false;
        while curr != 0 {
            if overlapping_taskbars.contains(&curr) {
                taskbar_above = true;
                break;
            }
            curr = unsafe { GetWindow(curr, GW_HWNDPREV) };
        }

        if taskbar_above {
            request_reassert(app);
        }
    }

    /// Post at most one main-thread reassert at a time. Geometry probing stays
    /// on the worker; only the final HWND operation is dispatched to Tauri.
    fn request_reassert(app: &tauri::AppHandle) {
        if REASSERT_PENDING.swap(true, Ordering::AcqRel) {
            return;
        }
        let app_for_main = app.clone();
        if app
            .run_on_main_thread(move || {
                REASSERT_PENDING.store(false, Ordering::Release);
                if !FLOATBAR_ACTIVE.load(Ordering::Acquire) {
                    return;
                }
                let Some(floatbar) = app_for_main.get_webview_window(FLOATBAR_LABEL) else {
                    FLOATBAR_ACTIVE.store(false, Ordering::Release);
                    return;
                };
                if floatbar.is_visible().unwrap_or(false) {
                    window::apply_always_on_top(&floatbar);
                }
            })
            .is_err()
        {
            REASSERT_PENDING.store(false, Ordering::Release);
        }
    }

    fn taskbars() -> Vec<(isize, Rect)> {
        let primary_class = PRIMARY_CLASS.get_or_init(|| wide("Shell_TrayWnd"));
        let secondary_class = SECONDARY_CLASS.get_or_init(|| wide("Shell_SecondaryTrayWnd"));
        let mut list = Vec::new();

        let primary = unsafe { FindWindowW(primary_class.as_ptr(), std::ptr::null()) };
        push_taskbar(primary, &mut list);

        let mut previous = 0;
        loop {
            let taskbar =
                unsafe { FindWindowExW(0, previous, secondary_class.as_ptr(), std::ptr::null()) };
            if taskbar == 0 {
                break;
            }
            push_taskbar(taskbar, &mut list);
            previous = taskbar;
        }

        list
    }

    fn push_taskbar(hwnd: isize, list: &mut Vec<(isize, Rect)>) {
        if hwnd == 0 || unsafe { IsWindowVisible(hwnd) } == 0 {
            return;
        }
        let mut rect = Rect::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) } != 0 {
            list.push((hwnd, rect));
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
        fn FindWindowExW(
            parent: isize,
            child_after: isize,
            class_name: *const u16,
            window_name: *const u16,
        ) -> isize;
        fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetWindow(hwnd: isize, ucmd: u32) -> isize;
    }
}

#[cfg(windows)]
pub fn install(app: &tauri::AppHandle) {
    platform::install(app);
}

#[cfg(windows)]
pub fn set_active(active: bool) {
    platform::set_active(active);
}

#[cfg(not(windows))]
pub fn install(_app: &tauri::AppHandle) {}

#[cfg(not(windows))]
pub fn set_active(_active: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_requires_positive_area() {
        let taskbar = Rect {
            left: 0,
            top: 1040,
            right: 1920,
            bottom: 1080,
        };
        assert!(rects_overlap(
            Rect {
                left: 800,
                top: 1044,
                right: 1120,
                bottom: 1076,
            },
            taskbar
        ));
        assert!(!rects_overlap(
            Rect {
                left: 800,
                top: 1000,
                right: 1120,
                bottom: 1040,
            },
            taskbar
        ));
    }
}
