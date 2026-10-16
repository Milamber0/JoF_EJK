//! `tell <name> <message>`: name the recipient instead of its slot.
//!
//! The stock server's `Cmd_Tell_f` only takes a slot or an exact name. Like
//! EternalJK's `CG_Say_f` (`codemp/cgame/cg_consolecmds.c:2021`), the client
//! resolves a name or unique partial name from the player list first and sends
//! `tell <slot> <message>`; when nobody or several players match it prints why
//! and sends nothing. A numeric target is passed through unchanged.

use sjk_client::{ChatDestination, ChatRoster, PlayerLookup, chat_command};

/// What to do with a forwarded console command.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Tell {
    /// Not a `tell` by name: forward the command as typed.
    Unchanged,
    /// Forward this rewritten `tell <slot> "<message>"` instead.
    Send(String),
    /// Send nothing; print these lines.
    Refused(Vec<String>),
}

/// Resolve the recipient of a tokenized `tell` against the current roster.
pub(super) fn resolve(tokens: &[String], roster: &ChatRoster) -> Tell {
    let [name, target, message @ ..] = tokens else {
        return Tell::Unchanged;
    };
    // Without a message the server prints its usage; a number is already a slot.
    if !name.eq_ignore_ascii_case("tell")
        || message.is_empty()
        || target.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Tell::Unchanged;
    }
    match roster.lookup(target) {
        PlayerLookup::Found(slot) => {
            chat_command(ChatDestination::Player(slot), &message.join(" "))
                .map_or(Tell::Unchanged, Tell::Send)
        }
        PlayerLookup::NotFound => Tell::Refused(vec![format!(
            "^3tell: no player matches \"{target}^3\"; nothing sent"
        )]),
        PlayerLookup::Ambiguous(candidates) => {
            let mut lines = Vec::with_capacity(candidates.len() + 1);
            lines.push(format!(
                "^3tell: \"{target}^3\" matches several players; nothing sent. Use a number or more of the name:"
            ));
            lines.extend(
                candidates
                    .into_iter()
                    .map(|(slot, name)| format!("  {slot:2}: {name}^7")),
            );
            Tell::Refused(lines)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(line: &str) -> Vec<String> {
        sjk_shell::tokenize(line).unwrap()
    }

    #[test]
    fn numbers_and_other_commands_are_forwarded_unchanged() {
        let roster = ChatRoster::default();
        assert_eq!(resolve(&tokens("tell 3 hi"), &roster), Tell::Unchanged);
        assert_eq!(resolve(&tokens("say bob hi"), &roster), Tell::Unchanged);
        assert_eq!(resolve(&tokens("tell bob"), &roster), Tell::Unchanged);
    }

    #[test]
    fn unknown_name_sends_nothing() {
        let roster = ChatRoster::default();
        let Tell::Refused(lines) = resolve(&tokens("tell bob hi there"), &roster) else {
            panic!("an empty roster must refuse a name");
        };
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("bob"));
    }
}
