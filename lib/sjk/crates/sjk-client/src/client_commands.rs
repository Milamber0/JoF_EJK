//! Profile-owned console metadata for server-side client commands.
//!
//! TaystJK's authoritative dispatch table is
//! `codemp/game/g_cmds.c:8864-9082` at commit 5802c999. Only harmless
//! cosmetic/emote and information/query commands are advertised here. The
//! server remains the sole command implementation and permission authority.

use crate::CompatProfile;

/// A server command advertised by the active compatibility profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatConsoleCommand {
    /// Case-insensitive command token accepted by the server.
    pub name: &'static str,
    /// Short console-help description.
    pub description: &'static str,
}

const fn command(name: &'static str, description: &'static str) -> CompatConsoleCommand {
    CompatConsoleCommand { name, description }
}

// Emote handlers are dispatched at g_cmds.c:8868-8920 and individually
// gated by g_emotesDisable in their handlers (g_cmds.c:6340-6713).
const TAYSTJK_EMOTES: &[CompatConsoleCommand] = &[
    command("ambeg", "TaystJK beg emote"),
    command("ambeg2", "TaystJK alternate beg emote"),
    command("ambernie", "TaystJK Bernie emote"),
    command("ambreakdance", "TaystJK breakdance emote"),
    command("ambreakdance2", "TaystJK alternate breakdance emote"),
    command("ambreakdance3", "TaystJK alternate breakdance emote"),
    command("ambreakdance4", "TaystJK alternate breakdance emote"),
    command("amcheer", "TaystJK cheer emote"),
    command("amcower", "TaystJK cower emote"),
    command("amdance", "TaystJK dance emote"),
    command("amflip", "TaystJK saber-flip emote"),
    command("amhug", "TaystJK hug emote"),
    command("amnoisy", "TaystJK noisy emote"),
    command("ampoint", "TaystJK point emote"),
    command("amrage", "TaystJK rage emote"),
    command("amsignal", "TaystJK hand-signal emote"),
    command("amsignal2", "TaystJK alternate hand-signal emote"),
    command("amsignal3", "TaystJK alternate hand-signal emote"),
    command("amsignal4", "TaystJK alternate hand-signal emote"),
    command("amsit", "TaystJK sit emote"),
    command("amsit2", "TaystJK alternate sit emote"),
    command("amsit3", "TaystJK alternate sit emote"),
    command("amsit4", "TaystJK alternate sit emote"),
    command("amsit5", "TaystJK alternate sit emote"),
    command("amslap", "TaystJK slap emote"),
    command("amsleep", "TaystJK sleep emote"),
    command("amsmack", "TaystJK smack emote"),
    command("amsurrender", "TaystJK surrender emote"),
    command("amtaunt", "TaystJK taunt emote"),
    command("amtaunt2", "TaystJK alternate taunt emote"),
    command("amvictory", "TaystJK victory emote"),
];

// Read-only/status handlers in the same dispatch table. Commands which mutate
// movement, combat, accounts, clans, or server state are intentionally absent.
const TAYSTJK_INFO: &[CompatConsoleCommand] = &[
    command("aminfo", "Show TaystJK server command help"),
    command("serverconfig", "List the server's options"),
    command("ammotd", "Show the server message of the day"),
    command("claninfo", "Show clan information"),
    command("clanlist", "List registered clans"),
    command("clanwhois", "Show a player's clan identity"),
    command("masterlist", "List server account masters"),
    command("modversion", "Show the server TaystJK build"),
    command("printstats", "Print current run statistics"),
    command("rcompare", "Compare race records"),
    command("rfind", "Find race records"),
    command("rhardest", "List hardest race records"),
    command("rlatest", "List recent race records"),
    command("rpopular", "List popular race records"),
    command("rrank", "Show race rank"),
    command("rtop", "List top race records"),
    command("rworst", "List race records to improve"),
    command("shownet", "Show player network settings"),
    command("stats", "Show account statistics"),
    command("top", "Show duel leaderboard"),
    command("warplist", "List available race warps"),
    command("whois", "Show account identity"),
];

/// Iterate commands suitable for console help/completion in this profile.
pub fn console_commands(
    profile: &CompatProfile,
) -> impl Iterator<Item = CompatConsoleCommand> + '_ {
    let enabled = matches!(profile, CompatProfile::TaystJk);
    TAYSTJK_EMOTES
        .iter()
        .chain(TAYSTJK_INFO)
        .copied()
        .filter(move |_| enabled)
}
