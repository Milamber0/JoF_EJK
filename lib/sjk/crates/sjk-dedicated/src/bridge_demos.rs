//! Server demos on this server: the files the endpoint records clients' messages into
//! (`demos/<name>.dm_26` in the home directory's `base`).

use super::NativeGame;
use std::io::Write;

/// `strftime("%Y-%m-%d_%H-%M-%S")` of a moment, in UTC (the reference uses the local
/// time zone, which a headless server may not know).
pub(super) fn timestamp(since_epoch: std::time::Duration) -> String {
    let seconds = since_epoch.as_secs() as i64;
    let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}-{:02}-{:02}",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

impl NativeGame {
    /// Open a demo file for a client; `false` without a home directory or where it
    /// cannot be created.
    pub(super) fn open_demo(&mut self, client: usize, path: &str) -> bool {
        let Some(file) = self.config_files.create_home(path) else {
            return false;
        };
        if self.demo_files.len() <= client {
            self.demo_files.resize_with(client + 1, || None);
        }
        self.demo_files[client] = Some(file);
        true
    }

    /// Bytes for a client's demo file.
    pub(super) fn write_demo(&mut self, client: usize, bytes: &[u8]) {
        if let Some(Some(file)) = self.demo_files.get_mut(client)
            && file.write_all(bytes).is_err()
        {
            println!("couldn't write client {client}'s demo");
        }
    }

    /// A client's demo file is complete.
    pub(super) fn close_demo(&mut self, client: usize) {
        if let Some(file) = self.demo_files.get_mut(client) {
            *file = None;
        }
    }
}
