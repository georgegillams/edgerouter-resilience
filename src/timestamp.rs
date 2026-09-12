//! Local-time formatting for log lines and scheduled checks.

use chrono::{Local, Timelike};

/// Current local time, formatted for log prefixes.
pub fn now_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Local hour and minute (24-hour clock).
pub fn local_hour_minute() -> (u32, u32) {
    let now = Local::now();
    (now.hour(), now.minute())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_string_is_iso_like() {
        let stamp = now_string();
        assert_eq!(stamp.len(), 19);
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[7..8], "-");
        assert_eq!(&stamp[10..11], " ");
        assert_eq!(&stamp[13..14], ":");
        assert_eq!(&stamp[16..17], ":");
    }

    #[test]
    fn local_hour_minute_is_in_range() {
        let (hour, minute) = local_hour_minute();
        assert!(hour < 24);
        assert!(minute < 60);
    }
}

