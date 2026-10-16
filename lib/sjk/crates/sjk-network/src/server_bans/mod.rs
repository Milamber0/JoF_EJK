//! The engine's ban list (`sv_ccmds.cpp:575-1080`, `sv_client.cpp:SV_IsBanned`): bans and
//! exceptions by IPv4 address and prefix, kept in a file between runs, checked when a
//! client connects.
//!
//! Addresses are read as the reference reads them (`NET_StringToAdr` over
//! `inet_addr`, so `10.1` and octal parts are addresses), and a line of the ban file the
//! reference cannot read leaves what that slot held before, as the reference's fixed
//! array does.

use std::net::{Ipv4Addr, SocketAddrV4};
mod address;
pub use address::{legacy_inet_addr, legacy_string_to_address};

/// `SERVER_MAXBANS`: the reference's list holds this many; the list here holds as many
/// as it is built for.
pub const LEGACY_DEFAULT_MAX_BANS: usize = 1024;
/// `PORT_SERVER`, the port an address without one is given.
const PORT_SERVER: u16 = 29070;

/// An address as a ban entry holds it (`netadr_t`): read, or `BAD` for a file line the
/// reference could not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyBanAddress {
    /// `NA_IP`, with the port it was given.
    Ip(SocketAddrV4),
    /// `NA_LOOPBACK`: `localhost`.
    Loopback,
    /// `NA_BAD`.
    Bad,
}

impl LegacyBanAddress {
    /// `NET_AdrToString`.
    pub fn to_text(self) -> String {
        match self {
            Self::Ip(address) => address.to_string(),
            Self::Loopback => "loopback".to_owned(),
            Self::Bad => "BAD".to_owned(),
        }
    }
}

/// One ban or exception (`serverBan_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyBan {
    pub address: LegacyBanAddress,
    /// The prefix length, 1 to 32.
    pub subnet: i32,
    pub exception: bool,
}

/// The list. Entries past [`Self::len`] are what the reference's array still holds there.
#[derive(Clone, Debug)]
pub struct LegacyBanList {
    entries: Vec<LegacyBan>,
    count: usize,
    capacity: usize,
}

impl Default for LegacyBanList {
    fn default() -> Self {
        Self::new(LEGACY_DEFAULT_MAX_BANS)
    }
}

/// `NET_CompareBaseAdrMask` for two IPv4 addresses (ports ignored).
fn same_prefix(a: Ipv4Addr, b: Ipv4Addr, mask: i32) -> bool {
    let mask = (mask as u32).min(32);
    if mask == 0 {
        return true;
    }
    let bits = u32::MAX << (32 - mask);
    u32::from(a) & bits == u32::from(b) & bits
}

impl LegacyBanList {
    /// An empty list that holds at most `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            count: 0,
            capacity,
        }
    }

    /// How many bans and exceptions there are.
    pub fn len(&self) -> usize {
        self.count
    }
    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// The bans and exceptions, in order.
    pub fn entries(&self) -> &[LegacyBan] {
        &self.entries[..self.count]
    }

    fn matches(ban: &LegacyBan, address: LegacyBanAddress, mask: i32) -> bool {
        match (ban.address, address) {
            (LegacyBanAddress::Ip(a), LegacyBanAddress::Ip(b)) => {
                same_prefix(*a.ip(), *b.ip(), mask)
            }
            (LegacyBanAddress::Loopback, LegacyBanAddress::Loopback) => true,
            _ => false,
        }
    }

    /// `SV_IsBanned`: banned unless an exception covers it.
    pub fn is_banned(&self, from: SocketAddrV4) -> bool {
        let from = LegacyBanAddress::Ip(from);
        let covered = |exception: bool| {
            self.entries()
                .iter()
                .any(|ban| ban.exception == exception && Self::matches(ban, from, ban.subnet))
        };
        !covered(true) && covered(false)
    }

    /// `SV_DelBanEntryFromList`: the entries after it move down; the last stays behind
    /// in the array.
    fn remove(&mut self, index: usize) {
        if index + 1 == self.count {
            self.count -= 1;
        } else if index + 1 < self.capacity {
            self.entries.copy_within(index + 1..self.count, index);
            self.count -= 1;
        }
    }

    fn store(&mut self, index: usize, ban: LegacyBan) {
        if index < self.entries.len() {
            self.entries[index] = ban;
        } else {
            self.entries.push(ban);
        }
    }

    /// `SV_AddBanToList` once the address and prefix are known: refused where an
    /// existing entry supersedes it, and every entry it supersedes removed. `Err` holds
    /// the message.
    pub fn add(&mut self, address: SocketAddrV4, mask: i32, exception: bool) -> Result<(), String> {
        let ip = LegacyBanAddress::Ip(address);
        for ban in self.entries() {
            if ban.subnet <= mask
                && (ban.exception || !exception)
                && Self::matches(ban, ip, ban.subnet)
            {
                return Err(format!(
                    "Error: {} {}/{} supersedes {} {}/{mask}\n",
                    if ban.exception { "Exception" } else { "Ban" },
                    ban.address.to_text(),
                    ban.subnet,
                    if exception { "exception" } else { "ban" },
                    ip.to_text()
                ));
            }
            if ban.subnet >= mask && !ban.exception && exception && Self::matches(ban, ip, mask) {
                return Err(format!(
                    "Error: {} {}/{mask} supersedes already existing {} {}/{}\n",
                    if exception { "Exception" } else { "Ban" },
                    ip.to_text(),
                    if ban.exception { "exception" } else { "ban" },
                    ban.address.to_text(),
                    ban.subnet
                ));
            }
        }
        let mut index = 0;
        while index < self.count {
            let ban = self.entries[index];
            if ban.subnet > mask && (!ban.exception || exception) && Self::matches(&ban, ip, mask) {
                self.remove(index);
            } else {
                index += 1;
            }
        }
        self.store(
            self.count,
            LegacyBan {
                address: ip,
                subnet: mask,
                exception,
            },
        );
        self.count += 1;
        Ok(())
    }

    /// Whether the list is full (`Maximum number of bans/exceptions exceeded`).
    pub fn is_full(&self) -> bool {
        self.count >= self.capacity
    }

    /// `SV_DelBanFromList` by address: every entry of the kind within the prefix, each
    /// told through `print`.
    pub fn remove_matching(
        &mut self,
        address: LegacyBanAddress,
        mask: i32,
        exception: bool,
        print: &mut dyn FnMut(&[u8]),
    ) {
        let mut index = 0;
        while index < self.count {
            let ban = self.entries[index];
            if ban.exception == exception
                && ban.subnet >= mask
                && Self::matches(&ban, address, mask)
            {
                print(
                    format!(
                        "Deleting {} {}/{}\n",
                        kind(exception),
                        ban.address.to_text(),
                        ban.subnet
                    )
                    .as_bytes(),
                );
                self.remove(index);
            } else {
                index += 1;
            }
        }
    }

    /// `SV_DelBanFromList` by number: the `number`th entry of the kind, counted from 1
    /// among its kind — refused unless 1 to the whole list's length.
    pub fn remove_numbered(&mut self, number: i32, exception: bool, print: &mut dyn FnMut(&[u8])) {
        if number < 1 || number as usize > self.count {
            print(b"Error: Invalid ban number given\n");
            return;
        }
        let found = (0..self.count)
            .filter(|&index| self.entries[index].exception == exception)
            .nth(number as usize - 1);
        if let Some(index) = found {
            let ban = self.entries[index];
            print(
                format!(
                    "Deleting {} {}/{}\n",
                    kind(exception),
                    ban.address.to_text(),
                    ban.subnet
                )
                .as_bytes(),
            );
            self.remove(index);
        }
    }

    /// `SV_ListBans_f`: the bans, then the exceptions, each numbered among its kind.
    pub fn list(&self, print: &mut dyn FnMut(&[u8])) {
        for (title, exception) in [("Ban", false), ("Except", true)] {
            for (number, ban) in self
                .entries()
                .iter()
                .filter(|ban| ban.exception == exception)
                .enumerate()
            {
                print(
                    format!(
                        "{title} #{}: {}/{}\n",
                        number + 1,
                        ban.address.to_text(),
                        ban.subnet
                    )
                    .as_bytes(),
                );
            }
        }
    }

    /// `SV_FlushBans_f`'s emptying.
    pub fn clear(&mut self) {
        self.count = 0;
    }

    /// `SV_WriteBans`: `exception address subnet` a line.
    pub fn to_file(&self) -> Vec<u8> {
        self.entries()
            .iter()
            .flat_map(|ban| {
                format!(
                    "{} {} {}\n",
                    u8::from(ban.exception),
                    ban.address.to_text(),
                    ban.subnet
                )
                .into_bytes()
            })
            .collect()
    }

    /// `SV_RehashBans_f`: the list read back from the file's text. Reading stops at a
    /// line without its end; a line whose address cannot be read keeps the slot's old
    /// contents but its address, which becomes `BAD`.
    pub fn read_file(&mut self, text: &[u8], resolve: &mut dyn FnMut(&str) -> Option<Ipv4Addr>) {
        self.count = 0;
        if text.len() < 2 {
            return;
        }
        let (mut position, mut index) = (0, 0);
        while index < self.capacity && position + 2 < text.len() {
            let Some(space) = text[position + 2..]
                .iter()
                .position(|&byte| byte == b' ')
                .map(|at| position + 2 + at)
            else {
                break;
            };
            if space + 1 >= text.len() {
                break;
            }
            let Some(newline) = text[space + 1..]
                .iter()
                .position(|&byte| byte == b'\n')
                .map(|at| space + 1 + at)
            else {
                break;
            };
            let address = String::from_utf8_lossy(&text[position + 2..space]).into_owned();
            let parsed = legacy_string_to_address(&address, resolve);
            let previous = self.entries.get(index).copied().unwrap_or(LegacyBan {
                address: LegacyBanAddress::Bad,
                subnet: 0,
                exception: false,
            });
            let entry = match parsed {
                LegacyBanAddress::Bad => LegacyBan {
                    address: LegacyBanAddress::Bad,
                    ..previous
                },
                read => {
                    let mut subnet = crate::server_bans::atoi(&text[space + 1..newline]);
                    if matches!(read, LegacyBanAddress::Ip(_)) && !(1..=32).contains(&subnet) {
                        subnet = 32;
                    }
                    LegacyBan {
                        address: read,
                        subnet,
                        exception: text[position] != b'0',
                    }
                }
            };
            self.store(index, entry);
            position = newline + 1;
            index += 1;
        }
        self.count = index;
    }
}

fn kind(exception: bool) -> &'static str {
    if exception { "exception" } else { "ban" }
}

/// `atoi`.
pub(crate) fn atoi_text(text: &[u8]) -> i32 {
    atoi(text)
}

fn atoi(text: &[u8]) -> i32 {
    let start = text
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    let text = &text[start..];
    let negative = text.first() == Some(&b'-');
    let digits = &text[usize::from(matches!(text.first(), Some(b'+' | b'-')))..];
    let value =
        digits
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .fold(0_i64, |value, digit| {
                value
                    .saturating_mul(10)
                    .saturating_add(i64::from(digit - b'0'))
            });
    (if negative { -value } else { value }) as i32
}

/// `SV_ParseCIDRNotation`: an address with an optional `/prefix` (1 to 32, else 32).
pub fn legacy_parse_cidr(
    text: &str,
    resolve: &mut dyn FnMut(&str) -> Option<Ipv4Addr>,
) -> Option<(LegacyBanAddress, i32)> {
    let (address, suffix) = match text.split_once('/') {
        Some((address, suffix)) => (address, Some(suffix)),
        None => (text, None),
    };
    let address = legacy_string_to_address(address, resolve);
    if address == LegacyBanAddress::Bad {
        return None;
    }
    let mask = match (suffix, address) {
        (Some(suffix), LegacyBanAddress::Ip(_)) => {
            let mask = atoi(suffix.as_bytes());
            if (1..=32).contains(&mask) { mask } else { 32 }
        }
        _ => 32,
    };
    Some((address, mask))
}
