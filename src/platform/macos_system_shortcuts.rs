use base::config::keys::{
    OPTION_SEND_SYSTEM_SHORTCUTS_TO_REMOTE, OPTION_SUSPENDED_SYSTEM_SHORTCUTS,
};
use hbb_common::{config::LocalConfig, libc, log};
use std::sync::Mutex;

// macOS handles Mission Control and Spaces shortcuts before any event tap sees
// them, so the grab loop cannot forward them to the remote. While a session owns
// the keyboard grab, they are turned off for this login session and turned back
// on when it releases the grab, as Jump Desktop does.
//
// Symbolic hotkey ids: Mission Control (32, 34), application windows (33, 35),
// show desktop (36, 37), move a space left/right (79-82), switch to desktop 1-16 (118-133).
const HOTKEYS: &[i32] = &[
    32, 33, 34, 35, 36, 37, 79, 80, 81, 82, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127,
    128, 129, 130, 131, 132, 133,
];

type SetEnabled = unsafe extern "C" fn(hotkey: i32, enabled: bool) -> i32;
type IsEnabled = unsafe extern "C" fn(hotkey: i32) -> bool;

struct Api {
    set_enabled: SetEnabled,
    is_enabled: IsEnabled,
}

static SUSPENDED: Mutex<Vec<i32>> = Mutex::new(Vec::new());

// Private SkyLight API, resolved at runtime so a macOS without it disables the feature.
fn api() -> Option<Api> {
    unsafe {
        let set = libc::dlsym(
            libc::RTLD_DEFAULT,
            b"CGSSetSymbolicHotKeyEnabled\0".as_ptr() as _,
        );
        let is = libc::dlsym(
            libc::RTLD_DEFAULT,
            b"CGSIsSymbolicHotKeyEnabled\0".as_ptr() as _,
        );
        if set.is_null() || is.is_null() {
            return None;
        }
        Some(Api {
            set_enabled: std::mem::transmute::<*mut libc::c_void, SetEnabled>(set),
            is_enabled: std::mem::transmute::<*mut libc::c_void, IsEnabled>(is),
        })
    }
}

pub fn suspend() {
    if LocalConfig::get_option(OPTION_SEND_SYSTEM_SHORTCUTS_TO_REMOTE) != "Y" {
        return;
    }
    let Some(api) = api() else {
        log::warn!("System shortcut API is unavailable");
        return;
    };
    let mut suspended = SUSPENDED.lock().unwrap();
    if !suspended.is_empty() {
        return;
    }
    for &hotkey in HOTKEYS {
        if unsafe { (api.is_enabled)(hotkey) } && unsafe { (api.set_enabled)(hotkey, false) } == 0
        {
            suspended.push(hotkey);
        }
    }
    if !suspended.is_empty() {
        // Recorded so a crash while suspended can be undone on the next start.
        LocalConfig::set_option(
            OPTION_SUSPENDED_SYSTEM_SHORTCUTS.to_owned(),
            join_hotkeys(&suspended),
        );
    }
}

pub fn restore() {
    let mut suspended = SUSPENDED.lock().unwrap();
    if suspended.is_empty() {
        return;
    }
    enable(&suspended);
    suspended.clear();
    LocalConfig::set_option(OPTION_SUSPENDED_SYSTEM_SHORTCUTS.to_owned(), "".to_owned());
}

pub fn restore_after_crash() {
    let hotkeys = parse_hotkeys(&LocalConfig::get_option(OPTION_SUSPENDED_SYSTEM_SHORTCUTS));
    if hotkeys.is_empty() || !SUSPENDED.lock().unwrap().is_empty() {
        return;
    }
    log::info!("Restoring system shortcuts left suspended: {:?}", hotkeys);
    enable(&hotkeys);
    LocalConfig::set_option(OPTION_SUSPENDED_SYSTEM_SHORTCUTS.to_owned(), "".to_owned());
}

fn enable(hotkeys: &[i32]) {
    let Some(api) = api() else {
        return;
    };
    for &hotkey in hotkeys {
        let err = unsafe { (api.set_enabled)(hotkey, true) };
        if err != 0 {
            log::error!("Failed to restore system shortcut {}: {}", hotkey, err);
        }
    }
}

fn join_hotkeys(hotkeys: &[i32]) -> String {
    hotkeys
        .iter()
        .map(|h| h.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

// Ignores ids outside `HOTKEYS` so a hand-edited value cannot enable unrelated shortcuts.
fn parse_hotkeys(value: &str) -> Vec<i32> {
    value
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .filter(|h| HOTKEYS.contains(h))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_hotkeys_round_trip_and_ignore_unknown_ids() {
        assert_eq!(parse_hotkeys(&join_hotkeys(&[32, 79, 119])), vec![32, 79, 119]);
        assert_eq!(parse_hotkeys("119, 64,abc,"), vec![119]);
        assert!(parse_hotkeys("").is_empty());
    }
}
