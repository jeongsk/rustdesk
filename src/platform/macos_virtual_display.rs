use base::config::keys::OPTION_ALLOW_MACOS_VIRTUAL_DISPLAY;
use hbb_common::{allow_err, bail, config::Config, log, ResultType};
use std::{ffi::c_void, sync::Mutex};

const DEFAULT_SIZE: (usize, usize) = (1920, 1080);
const MIN_SIZE: (usize, usize) = (640, 480);

extern "C" {
    fn MacVirtualDisplayCreate(width: u32, height: u32, display_id: *mut u32) -> *mut c_void;
    fn MacVirtualDisplayApply(display: *mut c_void, width: u32, height: u32) -> bool;
    fn MacVirtualDisplayDestroy(display: *mut c_void);
    fn MacIsOnlyHeadlessPlaceholderDisplay() -> bool;
}

struct VirtualDisplay {
    handle: *mut c_void,
    id: u32,
}

// The handle is an Objective-C object that is only used under `VIRTUAL_DISPLAY`'s lock.
unsafe impl Send for VirtualDisplay {}

impl Drop for VirtualDisplay {
    fn drop(&mut self) {
        unsafe { MacVirtualDisplayDestroy(self.handle) };
    }
}

static VIRTUAL_DISPLAY: Mutex<Option<VirtualDisplay>> = Mutex::new(None);

// Replaces the fixed 1920x1080 placeholder macOS shows when no monitor is
// attached with a display whose resolution follows the controlling side.
pub fn plug_in_if_headless() {
    if !Config::get_bool_option(OPTION_ALLOW_MACOS_VIRTUAL_DISPLAY) {
        return;
    }
    let mut lock = VIRTUAL_DISPLAY.lock().unwrap();
    if lock.is_some() || !unsafe { MacIsOnlyHeadlessPlaceholderDisplay() } {
        return;
    }
    let mut id = 0;
    let handle =
        unsafe { MacVirtualDisplayCreate(DEFAULT_SIZE.0 as _, DEFAULT_SIZE.1 as _, &mut id) };
    if handle.is_null() {
        log::error!("Failed to create macOS virtual display");
        return;
    }
    log::info!("Created macOS virtual display {}", id);
    let display = VirtualDisplay { handle, id };
    allow_err!(select_mode(&display, DEFAULT_SIZE.0, DEFAULT_SIZE.1));
    *lock = Some(display);
}

pub fn plug_out() {
    if let Some(display) = VIRTUAL_DISPLAY.lock().unwrap().take() {
        log::info!("Removing macOS virtual display {}", display.id);
    }
}

pub fn is_my_display(name: &str) -> bool {
    VIRTUAL_DISPLAY
        .lock()
        .unwrap()
        .as_ref()
        .map_or(false, |d| d.id.to_string() == name)
}

// `None` if `name` is not the virtual display, so the caller keeps its normal path.
pub fn change_resolution(name: &str, width: usize, height: usize) -> Option<ResultType<()>> {
    let lock = VIRTUAL_DISPLAY.lock().unwrap();
    let display = lock.as_ref().filter(|d| d.id.to_string() == name)?;
    Some(apply(display, width, height))
}

fn apply(display: &VirtualDisplay, width: usize, height: usize) -> ResultType<()> {
    if !is_valid_size(width, height) {
        bail!("Invalid virtual display size {}x{}", width, height);
    }
    if !unsafe { MacVirtualDisplayApply(display.handle, width as _, height as _) } {
        bail!("Failed to apply virtual display size {}x{}", width, height);
    }
    select_mode(display, width, height)
}

// `applySettings` adds both a 1x and a 2x mode of the size and may keep the 1x one active.
fn select_mode(display: &VirtualDisplay, width: usize, height: usize) -> ResultType<()> {
    super::macos::change_resolution_directly(&display.id.to_string(), width, height)
}

fn is_valid_size(width: usize, height: usize) -> bool {
    width >= MIN_SIZE.0 && height >= MIN_SIZE.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_sizes_below_minimum() {
        assert!(is_valid_size(1512, 982));
        assert!(!is_valid_size(0, 0));
        assert!(!is_valid_size(320, 982));
    }
}
