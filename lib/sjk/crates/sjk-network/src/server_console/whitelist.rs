//! `whitelistip` (`SV_WhitelistIP_f`).
use super::{Arguments, LegacyConsoleHost};
use crate::{LegacyBanAddress, legacy_string_to_address};

/// `SV_WhitelistIP_f`: each address named listed. A name is looked up; `localhost` is
/// told listed though only IPv4 addresses are kept.
pub(super) fn command(
    host: &mut impl LegacyConsoleHost,
    arguments: &Arguments,
    print: &mut dyn FnMut(&[u8]),
) {
    let count = arguments.count();
    if count < 2 {
        print(b"Usage: whitelistip <ip>...\n");
        return;
    }
    for index in 1..count {
        let text = String::from_utf8_lossy(arguments.get(index)).into_owned();
        let address = legacy_string_to_address(&text, &mut |name| host.resolve(name));
        match address {
            LegacyBanAddress::Bad => print(format!("Incorrect IP address: {text}\n").as_bytes()),
            LegacyBanAddress::Ip(ip) => {
                host.whitelist(*ip.ip(), print);
                print(format!("Added {} to the IP whitelist\n", address.to_text()).as_bytes());
            }
            LegacyBanAddress::Loopback => {
                print(format!("Added {} to the IP whitelist\n", address.to_text()).as_bytes())
            }
        }
    }
}
