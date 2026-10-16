//! The denial-of-service whitelist (`sv_main.cpp:625-686`, `SV_WhitelistIP_f`): the IPv4
//! addresses of players who have been in the game (`sv_autoWhitelist`) and of those an
//! operator names (`whitelistip`). When the server is flooded with out-of-band requests,
//! a whitelisted sender gets twice the global rate ([`crate::LegacyOobLimiter`]).
//!
//! The list is kept in `ipwhitelist.dat` in the server's own directory: each address's
//! four bytes, appended as it is added, read back whole records from the start.

use crate::LegacyPeerAddress;
use std::collections::HashSet;
use std::net::Ipv4Addr;

/// `WHITELIST_FILE`, in the server's own directory.
pub const WHITELIST_FILE: &str = "ipwhitelist.dat";

/// How many addresses a list holds unless built for another number. The reference's set
/// grows without limit.
pub const LEGACY_DEFAULT_WHITELIST_CAPACITY: usize = 65_536;

/// What became of an address to be listed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyWhitelisting {
    /// Listed already: nothing to append.
    Known,
    /// Listed now: its record is to be appended to the file.
    Added,
    /// The list is full and the address is not listed.
    Full,
}

/// The list.
#[derive(Clone, Debug)]
pub struct LegacyWhitelist {
    addresses: HashSet<Ipv4Addr>,
    capacity: usize,
}

impl Default for LegacyWhitelist {
    fn default() -> Self {
        Self::new(LEGACY_DEFAULT_WHITELIST_CAPACITY)
    }
}

impl LegacyWhitelist {
    /// An empty list of at most `capacity` addresses.
    pub fn new(capacity: usize) -> Self {
        Self {
            addresses: HashSet::new(),
            capacity,
        }
    }

    /// `SVC_LoadWhitelist`: every whole four-byte record of the file added; bytes after
    /// the last whole record are not read.
    pub fn load(&mut self, file: &[u8]) {
        for record in file.chunks_exact(4) {
            self.insert(Ipv4Addr::new(record[0], record[1], record[2], record[3]));
        }
    }

    /// `SVC_WhitelistAdr`'s listing.
    pub fn insert(&mut self, address: Ipv4Addr) -> LegacyWhitelisting {
        if self.addresses.contains(&address) {
            LegacyWhitelisting::Known
        } else if self.addresses.len() >= self.capacity {
            LegacyWhitelisting::Full
        } else {
            self.addresses.insert(address);
            LegacyWhitelisting::Added
        }
    }

    /// `SVC_WhitelistAdr`: listed, and its record handed to `append` for the file;
    /// `print` is told when `append` cannot open the file or the list is full.
    pub fn whitelist(
        &mut self,
        address: Ipv4Addr,
        append: impl FnOnce([u8; 4]) -> bool,
        print: &mut dyn FnMut(&[u8]),
    ) {
        match self.insert(address) {
            LegacyWhitelisting::Known => {}
            LegacyWhitelisting::Added => {
                if !append(address.octets()) {
                    print(format!("Couldn't open {WHITELIST_FILE}.\n").as_bytes());
                }
            }
            LegacyWhitelisting::Full => {
                print(format!("The IP whitelist is full: {address} is not listed.\n").as_bytes())
            }
        }
    }

    /// `SVC_IsWhitelisted`: every sender that is not an IPv4 address counts as listed.
    pub fn contains(&self, from: LegacyPeerAddress) -> bool {
        match from {
            LegacyPeerAddress::Ip(address) => self.addresses.contains(address.ip()),
            _ => true,
        }
    }

    /// How many addresses are listed.
    pub fn len(&self) -> usize {
        self.addresses.len()
    }
    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }
}
