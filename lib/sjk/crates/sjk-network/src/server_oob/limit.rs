//! `SVC_RateLimit` and the per-address bucket table, with a bounded table.
use crate::LegacyPeerAddress;
use std::{collections::HashMap, net::Ipv4Addr};

/// One leaky bucket, identical in arithmetic to the reference's `leakyBucket_t`.
#[derive(Clone, Copy, Debug, Default)]
struct Bucket {
    last_time: i32,
    burst: u16,
}

impl Bucket {
    /// Whether this request exceeds `burst` requests per `burst * period` ms.
    /// A clock that runs backwards empties the bucket.
    fn limit(&mut self, burst: i32, period: i32, now: i32) -> bool {
        let interval = now.wrapping_sub(self.last_time);
        let expired = interval / period;
        if expired > i32::from(self.burst) || interval < 0 {
            self.burst = 0;
            self.last_time = now;
        } else {
            self.burst -= expired as u16;
            self.last_time = now.wrapping_sub(interval % period);
        }
        if i32::from(self.burst) < burst {
            self.burst += 1;
            return false;
        }
        true
    }

    /// The reference's collection test: drained long enough ago, or from the future.
    fn is_idle(&self, period: i32, now: i32) -> bool {
        let interval = now.wrapping_sub(self.last_time);
        interval > i32::from(self.burst).wrapping_mul(period) || interval < 0
    }
}

/// `sv_maxOOBRateIP` and `sv_maxOOBRate`, requests per second; zero disables one.
#[derive(Clone, Copy, Debug)]
pub struct LegacyOobRates {
    /// Per source address, burst of ten seconds' worth. Clamped to 1..=1000.
    pub per_address: i32,
    /// For all senders together, burst of one second's worth, doubled for
    /// whitelisted senders. Clamped to 1..=1000.
    pub global: i32,
}

/// Whether an out-of-band request may be handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyOobAdmission {
    /// Handle the request.
    Accepted,
    /// This address sent too many requests.
    AddressLimited,
    /// The address table is full of active senders and this one is new. The
    /// reference has no such case because its table grows without bound.
    AddressTableFull,
    /// All senders together sent too many requests.
    GlobalLimited,
}

/// Both out-of-band rate limiters of one server.
///
/// The reference keeps a bucket per source address in a map that grows with every
/// new address until a once-per-second collection drops idle ones, so a spoofed
/// flood costs memory in proportion to its packet rate. Here the table is
/// allocated once for `address_capacity` senders. When it is full of buckets that
/// are still active, requests from *new* addresses are refused while known senders
/// continue unaffected; below that point behaviour is the reference's.
pub struct LegacyOobLimiter {
    addresses: HashMap<Ipv4Addr, Bucket>,
    address_capacity: usize,
    last_collection: i32,
    global: Bucket,
    dropped: u32,
    last_report: i32,
}

impl LegacyOobLimiter {
    /// Allocate the address table once; it never grows.
    pub fn new(address_capacity: usize) -> Self {
        Self {
            addresses: HashMap::with_capacity(address_capacity),
            address_capacity,
            last_collection: 0,
            global: Bucket::default(),
            dropped: 0,
            last_report: 0,
        }
    }

    /// Decide one request at `now` (wall-clock milliseconds, not server time).
    ///
    /// Only IPv4 senders have address buckets; every other sender counts as
    /// whitelisted. A request the address limiter refuses never reaches the
    /// global bucket.
    pub fn admit(
        &mut self,
        from: LegacyPeerAddress,
        whitelisted: bool,
        rates: LegacyOobRates,
        now: i32,
    ) -> LegacyOobAdmission {
        if rates.per_address != 0
            && let LegacyPeerAddress::Ip(address) = from
        {
            let rate = rates.per_address.clamp(1, 1000);
            let (burst, period) = (10 * rate, 1000 / rate);
            if self.last_collection.wrapping_add(1000) < now {
                self.last_collection = now;
                self.addresses
                    .retain(|_, bucket| !bucket.is_idle(period, now));
            }
            let known = self.addresses.contains_key(address.ip());
            if !known && self.addresses.len() >= self.address_capacity {
                return LegacyOobAdmission::AddressTableFull;
            }
            if self
                .addresses
                .entry(*address.ip())
                .or_default()
                .limit(burst, period, now)
            {
                return LegacyOobAdmission::AddressLimited;
            }
        }
        if rates.global != 0 {
            let rate = rates.global.clamp(1, 1000);
            let trusted = whitelisted || !matches!(from, LegacyPeerAddress::Ip(_));
            let burst = if trusted { rate * 2 } else { rate };
            if self.global.limit(burst, 1000 / rate, now) {
                self.dropped = self.dropped.saturating_add(1);
                return LegacyOobAdmission::GlobalLimited;
            }
        }
        LegacyOobAdmission::Accepted
    }

    /// Requests the global limiter refused, reported at most every five seconds.
    /// Call after an accepted request, as the reference logs there.
    pub fn take_dropped_report(&mut self, now: i32) -> Option<u32> {
        if self.dropped == 0 || self.last_report.wrapping_add(5000) >= now {
            return None;
        }
        self.last_report = now;
        Some(std::mem::take(&mut self.dropped))
    }

    /// Senders that currently hold an address bucket.
    pub fn tracked_addresses(&self) -> usize {
        self.addresses.len()
    }
}
