//! Who is sent what, and when: `SV_UserinfoChanged`, `SV_UpdateUserinfo_f`,
//! `SV_RateMsec` and the scheduling half of `SV_SendMessageToClient`.
use super::Slot;
use crate::{LegacyClientPhase, LegacyInfoString, LegacyPeerAddress};
use std::net::Ipv4Addr;

/// `MAX_NAME_LENGTH`, terminator included.
const NAME_BYTES: usize = 32;
/// `HEADER_RATE_BYTES`: our header, the IP header and some overhead.
const HEADER_RATE_BYTES: i32 = 48;
/// `INFO_CHANGE_MIN_INTERVAL` and `INFO_CHANGE_MAX_COUNT`.
const INFO_CHANGE_INTERVAL: i32 = 6000;
const INFO_CHANGE_COUNT: i32 = 3;

/// The reference's rate and snapshot-rate cvars.
#[derive(Clone, Debug)]
pub struct LegacyRateSettings {
    /// `sv_lanForceRate`: LAN clients are neither rate limited nor paced by `snaps`.
    pub lan_force_rate: bool,
    /// Which addresses count as LAN. The reference also accepts the class-C
    /// networks of its own interfaces, which only the platform layer knows.
    pub is_lan: fn(Ipv4Addr) -> bool,
    /// `sv_ratePolicy`: 1 gives everyone [`Self::client_rate`], 2 honours the
    /// client's `rate` within the limits.
    pub rate_policy: i32,
    /// `sv_clientRate`, bytes per second.
    pub client_rate: i32,
    /// `sv_minRate`; zero for none. Repaired to 1,000 when first used below that.
    pub min_rate: i32,
    /// `sv_maxRate`; zero for none. Repaired to 1,000 when first used below that.
    pub max_rate: i32,
    /// `sv_snapsPolicy`: 1 gives everyone `fps`, 2 honours the client's `snaps`
    /// within the limits, 0 leaves the interval alone.
    pub snaps_policy: i32,
    /// `sv_snapsMin`.
    pub snaps_min: i32,
    /// `sv_snapsMax`.
    pub snaps_max: i32,
    /// `sv_fps`: server frames per second.
    pub fps: i32,
}

impl Default for LegacyRateSettings {
    /// OpenJK's defaults.
    fn default() -> Self {
        Self {
            lan_force_rate: true,
            is_lan: legacy_is_private_address,
            rate_policy: 1,
            client_rate: 50_000,
            min_rate: 0,
            max_rate: 0,
            snaps_policy: 1,
            snaps_min: 10,
            snaps_max: 40,
            fps: 40,
        }
    }
}

/// The fixed ranges of the reference's `Sys_IsLANAddress`: RFC 1918 and 127/8.
pub fn legacy_is_private_address(address: Ipv4Addr) -> bool {
    let [a, b, ..] = address.octets();
    a == 10 || a == 127 || (a == 172 && b & 0xf0 == 16) || (a == 192 && b == 168)
}

/// What became of a `userinfo` command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UserinfoUpdate {
    /// Accepted; tell the game.
    Changed,
    /// A third change within six seconds: answer `print "@@@TOO_MANY_INFO\n"\n`.
    /// The new string is kept all the same, unapplied, as the reference keeps it.
    TooMany,
    /// No argument.
    Ignored,
    /// The string has no room for the server's `ip` key: drop with this reason.
    Drop(&'static [u8]),
}

impl Slot {
    fn is_lan(&self, rates: &LegacyRateSettings) -> bool {
        match self.peer.address {
            Some(LegacyPeerAddress::Loopback) => true,
            Some(LegacyPeerAddress::Ip(address)) => (rates.is_lan)(*address.ip()),
            _ => false,
        }
    }

    /// Apply the slot's userinfo: name, rate, snapshot interval and the `ip` key.
    pub(super) fn userinfo_changed(
        &mut self,
        rates: &LegacyRateSettings,
    ) -> Result<(), &'static [u8]> {
        let value = |key: &[u8]| self.userinfo.value(key).unwrap_or_default();
        self.name.clear();
        self.name.extend_from_slice(value(b"name"));
        self.name.truncate(NAME_BYTES - 1);

        if rates.lan_force_rate && self.is_lan(rates) {
            self.rate = 100_000;
        } else if rates.rate_policy == 1 {
            self.rate = rates.client_rate;
        } else if rates.rate_policy == 2 {
            let wish = match crate::connect_request::decimal(value(b"rate")) {
                0 => rates.max_rate,
                wish => wish,
            };
            // With neither limit set this clamps to zero, as it does in the reference.
            self.rate = clamp(rates.min_rate, rates.max_rate, clamp(1000, 100_000, wish));
        }

        let least = clamp(1, rates.snaps_max, rates.snaps_min);
        let most = rates.fps.min(rates.snaps_max);
        self.wish_snaps = match crate::connect_request::decimal(value(b"snaps")) {
            0 => most,
            wish => wish,
        };
        let interval = match rates.snaps_policy {
            1 => {
                self.wish_snaps = rates.fps;
                Some(1000 / rates.fps.max(1))
            }
            2 => Some(1000 / clamp(least, most, self.wish_snaps).max(1)),
            _ => None,
        };
        if let Some(interval) = interval.filter(|&interval| interval != self.snapshot_msec) {
            // A new interval takes effect with the very next frame.
            (self.next_snapshot_time, self.snapshot_msec) = (-1, interval);
        }

        let ip = match self.peer.address {
            Some(LegacyPeerAddress::Ip(address)) => address.to_string(),
            _ => "localhost".to_owned(),
        };
        let current = value(b"ip").len();
        let length = if current > 0 {
            ip.len() + self.userinfo.as_bytes().len() - current
        } else {
            ip.len() + 4 + self.userinfo.as_bytes().len()
        };
        if length >= 1024 {
            return Err(b"userinfo string length exceeded");
        }
        self.userinfo.set(b"ip", ip.as_bytes());
        Ok(())
    }

    /// The `userinfo` client command, with the release build's change limit.
    pub(super) fn update_userinfo(
        &mut self,
        argument: &[u8],
        now: i32,
        rates: &LegacyRateSettings,
    ) -> UserinfoUpdate {
        if argument.is_empty() {
            return UserinfoUpdate::Ignored;
        }
        self.userinfo = LegacyInfoString::from_truncated(argument);
        if self.last_userinfo_change > now {
            self.last_userinfo_count += 1;
            if self.last_userinfo_count >= INFO_CHANGE_COUNT {
                return UserinfoUpdate::TooMany;
            }
        } else {
            self.last_userinfo_count = 0;
            self.last_userinfo_change = now.wrapping_add(INFO_CHANGE_INTERVAL);
        }
        match self.userinfo_changed(rates) {
            Ok(()) => UserinfoUpdate::Changed,
            Err(reason) => UserinfoUpdate::Drop(reason),
        }
    }

    /// `SV_RateMsec`: milliseconds a message of `bytes` occupies this client's rate.
    ///
    /// Limits below 1,000 are repaired in the settings on first use, as the
    /// reference rewrites its cvars. A rate of zero, which rate policy 2 produces
    /// when no limit is set, makes the reference divide by zero; here it means no
    /// rate limit.
    fn rate_msec(&self, bytes: usize, rates: &mut LegacyRateSettings) -> i32 {
        let mut rate = self.rate;
        if rates.max_rate != 0 {
            rates.max_rate = rates.max_rate.max(1000);
            rate = rate.min(rates.max_rate);
        }
        if rates.min_rate != 0 {
            rates.min_rate = rates.min_rate.max(1000);
            rate = rate.max(rates.min_rate);
        }
        if rate == 0 {
            return 0;
        }
        (bytes.min(1500) as i32 + HEADER_RATE_BYTES) * 1000 / rate
    }

    /// Whether the scheduler lets this client send at `now`.
    pub(super) fn send_due(&self, now: i32) -> bool {
        self.phase != LegacyClientPhase::Free && now >= self.next_snapshot_time
    }

    /// A fragment is about to follow: pace it by the bytes still unsent.
    pub(super) fn schedule_fragment(&mut self, now: i32, rates: &mut LegacyRateSettings) {
        let unsent = self.wire.channel.pending_bytes();
        self.next_snapshot_time = now.wrapping_add(self.rate_msec(unsent, rates));
    }

    /// A message of `bytes` was just handed to the channel.
    pub(super) fn schedule_message(
        &mut self,
        now: i32,
        bytes: usize,
        rates: &mut LegacyRateSettings,
    ) {
        if self.peer.address == Some(LegacyPeerAddress::Loopback)
            || (rates.lan_force_rate && self.is_lan(rates))
        {
            // Local clients get a snapshot every server frame.
            self.next_snapshot_time = now.wrapping_add(1000 / rates.fps.max(1));
            return;
        }
        let mut delay = self.rate_msec(bytes, rates);
        self.rate_delayed = delay >= self.snapshot_msec;
        if !self.rate_delayed {
            // Never more often than the client's snapshot interval, whatever its rate.
            delay = self.snapshot_msec;
        }
        self.next_snapshot_time = now.wrapping_add(delay);
        // Do not pile up empty snapshots on a client that is still connecting, unless
        // it is downloading.
        if self.phase != LegacyClientPhase::Active
            && !self.download.active()
            && self.next_snapshot_time < now.wrapping_add(1000)
        {
            self.next_snapshot_time = now.wrapping_add(1000);
        }
    }
}

/// `Com_Clampi`, which answers `min` when the bounds are crossed.
fn clamp(min: i32, max: i32, value: i32) -> i32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
