//! The game's IP filter (`g_svcmds.c:30-230`): `addip`/`removeip`/`listip`, kept in the
//! `g_banIPs` variable and read from it when the game starts, and `G_FilterPacket`,
//! which `ClientConnect` asks before anything else ("Banned.").
//!
//! A filter is up to four dotted numbers, `*` matching any value in its place and
//! missing places matching anything; with `g_filterBan 0` the list admits instead of
//! refusing.

/// `MAX_IPFILTERS`: the reference's list holds this many; this one holds what it is
/// built for.
pub const DEFAULT_MAX_FILTERS: usize = 1024;
/// `MAX_CVAR_VALUE_STRING`: `g_banIPs` stops growing before this.
const BAN_IPS_BYTES: usize = 256;
/// A freed filter's `compare` (`0xFFFFFFFF`): skipped, and reused first.
const FREED: u32 = u32::MAX;

/// One filter (`ipFilter_t`): the bytes that must match and their values, in address
/// order (`byteAlias_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Filter {
    mask: [u8; 4],
    compare: [u8; 4],
}

impl Filter {
    fn compare_word(&self) -> u32 {
        u32::from_le_bytes(self.compare)
    }
}

/// The list.
#[derive(Clone, Debug)]
pub struct IpFilter {
    filters: Vec<Filter>,
    capacity: usize,
}

impl Default for IpFilter {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_FILTERS)
    }
}

/// `StringToFilter`: `None` (after "Bad filter address: …" through `print`) where a
/// place is neither a number nor `*`. Each number is taken as a byte.
fn string_to_filter(text: &[u8], print: &mut dyn FnMut(&[u8])) -> Option<Filter> {
    let (mut filter, mut at) = (
        Filter {
            mask: [0; 4],
            compare: [0; 4],
        },
        0,
    );
    for place in 0..4 {
        match text.get(at) {
            Some(byte) if byte.is_ascii_digit() => {
                let digits = text[at..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
                filter.compare[place] = crate::userinfo::atoi(&text[at..at + digits]) as u8;
                filter.mask[place] = 0xff;
                at += digits;
                if at >= text.len() {
                    break;
                }
                at += 1;
            }
            Some(b'*') => {
                at += 1;
                if at >= text.len() {
                    break;
                }
                at += 1;
            }
            _ => {
                print(
                    &[
                        &b"Bad filter address: "[..],
                        &text[at.min(text.len())..],
                        b"\n",
                    ]
                    .concat(),
                );
                return None;
            }
        }
    }
    Some(filter)
}

impl IpFilter {
    /// An empty list of at most `capacity` filters.
    pub fn new(capacity: usize) -> Self {
        Self {
            filters: Vec::new(),
            capacity,
        }
    }

    /// `G_ProcessIPBans`: every space-ended word of `g_banIPs` added; a last word
    /// without a space after it is not.
    pub fn process(&mut self, ban_ips: &[u8], print: &mut dyn FnMut(&[u8])) -> Vec<u8> {
        let mut rest = ban_ips;
        let mut ban_ips_now = ban_ips.to_vec();
        while let Some(space) = rest.iter().position(|&byte| byte == b' ') {
            let word = &rest[..space];
            rest = &rest[space..];
            rest = &rest[rest.iter().take_while(|&&byte| byte == b' ').count()..];
            if !word.is_empty() {
                ban_ips_now = self.add(word, print);
            }
        }
        ban_ips_now
    }

    /// `AddIP`: into the first freed place, or a new one if the list has room ("IP filter
    /// list is full"). A word that is not a filter takes its place as a freed one.
    /// Returns `g_banIPs`' new value.
    pub fn add(&mut self, text: &[u8], print: &mut dyn FnMut(&[u8])) -> Vec<u8> {
        let place = match self
            .filters
            .iter()
            .position(|filter| filter.compare_word() == FREED)
        {
            Some(place) => place,
            None => {
                if self.filters.len() == self.capacity {
                    print(b"IP filter list is full\n");
                    return self.ban_ips(print);
                }
                self.filters.push(Filter {
                    mask: [0; 4],
                    compare: [0; 4],
                });
                self.filters.len() - 1
            }
        };
        self.filters[place] = string_to_filter(text, print).unwrap_or(Filter {
            mask: self.filters[place].mask,
            compare: FREED.to_le_bytes(),
        });
        self.ban_ips(print)
    }

    /// `Svcmd_RemoveIP_f`: the filter written exactly the same way is freed ("Removed.");
    /// `Some` with `g_banIPs`' new value when one was.
    pub fn remove(&mut self, text: &[u8], print: &mut dyn FnMut(&[u8])) -> Option<Vec<u8>> {
        let wanted = string_to_filter(text, print)?;
        let Some(found) = self.filters.iter_mut().find(|filter| **filter == wanted) else {
            print(&[&b"Didn't find "[..], text, b".\n"].concat());
            return None;
        };
        found.compare = FREED.to_le_bytes();
        print(b"Removed.\n");
        Some(self.ban_ips(print))
    }

    /// `Svcmd_ListIP_f`: each filter's values (a `*` place as 0), then the count.
    pub fn list(&self, print: &mut dyn FnMut(&[u8])) {
        let live: Vec<&Filter> = self
            .filters
            .iter()
            .filter(|filter| filter.compare_word() != FREED)
            .collect();
        for filter in &live {
            let [a, b, c, d] = filter.compare;
            print(format!("{a}.{b}.{c}.{d}\n").as_bytes());
        }
        print(format!("{} bans.\n", live.len()).as_bytes());
    }

    /// `UpdateIPBans`: the filters written back as `g_banIPs`, each place a number or
    /// `*`, each filter followed by a space, as long as the value stays short enough.
    fn ban_ips(&self, print: &mut dyn FnMut(&[u8])) -> Vec<u8> {
        let mut out = Vec::new();
        for filter in self
            .filters
            .iter()
            .filter(|filter| filter.compare_word() != FREED)
        {
            let mut ip = String::new();
            for place in 0..4 {
                if filter.mask[place] != 0xff {
                    ip.push('*');
                } else {
                    ip.push_str(&filter.compare[place].to_string());
                }
                ip.push(if place < 3 { '.' } else { ' ' });
            }
            if out.len() + ip.len() < BAN_IPS_BYTES {
                out.extend_from_slice(ip.as_bytes());
            } else {
                print(b"g_banIPs overflowed at MAX_CVAR_VALUE_STRING\n");
                break;
            }
        }
        out
    }

    /// `G_FilterPacket`: whether a connection from `from` (the userinfo's `ip`, a port
    /// allowed) is refused. `filter_ban` is `g_filterBan`: refuse the listed, or admit
    /// only them.
    pub fn refuses(&self, from: &[u8], filter_ban: bool) -> bool {
        let mut address = [0_u8; 4];
        let (mut place, mut at) = (0, 0);
        while at < from.len() && place < 4 {
            address[place] = 0;
            while let Some(byte) = from.get(at).filter(|byte| byte.is_ascii_digit()) {
                address[place] = address[place].wrapping_mul(10).wrapping_add(byte - b'0');
                at += 1;
            }
            if at >= from.len() || from[at] == b':' {
                break;
            }
            place += 1;
            at += 1;
        }
        let word = u32::from_le_bytes(address);
        let listed = self
            .filters
            .iter()
            .any(|filter| word & u32::from_le_bytes(filter.mask) == filter.compare_word());
        if listed { filter_ban } else { !filter_ban }
    }
}
