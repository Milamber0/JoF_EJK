//! GIPHY links in an SJK chat message (`docs/hub-chat.md`, "GIFs from GIPHY"): which
//! words are links to a GIPHY GIF, the GIF's id they carry, and the one address SJK
//! fetches for it, rebuilt from the id on GIPHY's media host. The pasted address itself
//! is never fetched.
//!
//! Recognised (`https://`, or `http://`, which changes nothing as the address fetched
//! is rebuilt; the host compared whole, lower-cased; no user, port or other host):
//!
//! - `giphy.com/gifs/<slug>-<id>`, `giphy.com/gifs/<id>`, the same under `stickers/`,
//!   and `giphy.com/embed/<id>` (also `www.giphy.com`);
//! - `media.giphy.com/media/<id>/<file>` and `media0` to `media4.giphy.com`, with or
//!   without the `v1.<token>` segment GIPHY's share links put before the id
//!   (`media/v1.Y2lk.../<id>/giphy.gif?cid=...`);
//! - `i.giphy.com/<id>.gif` (or `.webp`) and `i.giphy.com/media/[v1.<token>/]<id>/<file>`.
//!
//! The query and fragment are ignored. Short links (`gph.is`) are not followed. An id
//! is GIPHY's: 6 to 40 ASCII letters and digits with at least one capital or digit (a
//! slug's last word such as `birthday` is not an id).

use std::ops::Range;

/// The address SJK's GIPHY requests go to: GIPHY's media host, and nothing else.
pub(crate) const MEDIA_HOST: &str = "media.giphy.com";

/// A GIPHY GIF's id, checked ([`GifId::new`]).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct GifId(String);

impl GifId {
    /// `value` as an id: 6 to 40 ASCII letters and digits, at least one a capital or a
    /// digit.
    pub(crate) fn new(value: &str) -> Option<Self> {
        let length = value.len();
        let valid = (6..=40).contains(&length)
            && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
            && value
                .bytes()
                .any(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit());
        valid.then(|| Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which of a GIF's files is fetched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Rendition {
    /// `200.gif`: 200 pixels high, as GIPHY makes for every GIF.
    Small,
    /// `giphy.gif`, the original, fetched only when the small one is not there.
    Original,
    /// `200w_d.gif`: 200 pixels wide at a lower frame rate, the GIF picker's tiles
    /// (a third of `200.gif`'s bytes, typically).
    Preview,
}

/// The one address fetched for `id`'s `rendition`, on [`MEDIA_HOST`].
pub(crate) fn media_url(id: &GifId, rendition: Rendition) -> String {
    let file = match rendition {
        Rendition::Small => "200.gif",
        Rendition::Original => "giphy.gif",
        Rendition::Preview => "200w_d.gif",
    };
    format!("https://{MEDIA_HOST}/media/{}/{file}", id.as_str())
}

/// Whether `url` is one [`media_url`] makes: `https://` on [`MEDIA_HOST`], a checked id
/// and one of the renditions' files. Checked again before every request.
pub(crate) fn fetchable(url: &str) -> bool {
    let Some(path) = url
        .strip_prefix("https://")
        .and_then(|rest| rest.strip_prefix(MEDIA_HOST))
        .and_then(|rest| rest.strip_prefix("/media/"))
    else {
        return false;
    };
    match path.split_once('/') {
        Some((id, file)) => {
            GifId::new(id).is_some() && matches!(file, "200.gif" | "giphy.gif" | "200w_d.gif")
        }
        None => false,
    }
}

/// The GIF id `word` links to, if it is a GIPHY link (see the module's list). Trailing
/// punctuation that ends a sentence (`.,!?;:)]>'"`) is not part of the link.
pub(crate) fn parse(word: &str) -> Option<GifId> {
    let word = trim_end(word);
    let lower = word
        .get(..8)
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let rest = if lower.starts_with("https://") {
        &word[8..]
    } else if lower.starts_with("http://") {
        &word[7..]
    } else {
        return None;
    };
    // The address up to its query or fragment.
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    // No user, no port: the host alone.
    if host.contains(['@', ':']) {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let segments: Vec<&str> = path.split('/').collect();
    // One trailing slash is allowed; empty segments elsewhere are not.
    let segments = match segments.split_last() {
        Some((&"", before)) => before,
        _ => &segments[..],
    };
    if segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    match host.as_str() {
        "giphy.com" | "www.giphy.com" => match segments {
            ["gifs" | "stickers", slug] => GifId::new(slug.rsplit('-').next()?),
            ["embed", id] => GifId::new(id),
            _ => None,
        },
        "media.giphy.com" | "media0.giphy.com" | "media1.giphy.com" | "media2.giphy.com"
        | "media3.giphy.com" | "media4.giphy.com" => media_path(segments),
        "i.giphy.com" => match segments {
            [file] => {
                let id = file
                    .strip_suffix(".gif")
                    .or_else(|| file.strip_suffix(".webp"))?;
                GifId::new(id)
            }
            _ => media_path(segments),
        },
        _ => None,
    }
}

/// `media/<id>/<file>` or `media/v1.<token>/<id>/<file>` on a media host.
fn media_path(segments: &[&str]) -> Option<GifId> {
    let id = match segments {
        ["media", id, file] if file_name(file) => id,
        ["media", token, id, file] if share_token(token) && file_name(file) => id,
        _ => return None,
    };
    GifId::new(id)
}

/// A media file's name: `giphy.gif`, `200w.webp`, `giphy-downsized.gif`...
fn file_name(file: &str) -> bool {
    (1..=48).contains(&file.len())
        && file.contains('.')
        && file
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The `v1.<token>` segment of GIPHY's share links (base64 of the share's ids).
fn share_token(token: &str) -> bool {
    token.strip_prefix("v1.").is_some_and(|rest| {
        (1..=256).contains(&rest.len())
            && rest
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'='))
    })
}

/// `word` without the punctuation that ends a sentence after a link.
fn trim_end(word: &str) -> &str {
    word.trim_end_matches(['.', ',', '!', '?', ';', ':', ')', ']', '>', '\'', '"'])
}

/// The first GIPHY link in `text`: its id and the bytes of the link (without the
/// punctuation after it). Words are split on spaces, as the chat's rules tidy them.
pub(crate) fn find(text: &str) -> Option<(GifId, Range<usize>)> {
    let mut start = 0;
    for word in text.split(' ') {
        let at = start;
        start += word.len() + 1;
        if let Some(id) = parse(word) {
            return Some((id, at..at + trim_end(word).len()));
        }
    }
    None
}

/// `text` with every GIPHY link written as its shortest address,
/// `https://giphy.com/gifs/<id>` (the punctuation after it kept): GIPHY's share links
/// carry a query that would take a message past the chat's length.
pub(crate) fn shortened(text: &str) -> String {
    text.split(' ')
        .map(|word| match parse(word) {
            Some(id) => format!("{}{}", page_url(&id), &word[trim_end(word).len()..]),
            None => word.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `id`'s shortest link, `https://giphy.com/gifs/<id>`: what a pasted link becomes and
/// what the GIF picker writes in the message.
pub(crate) fn page_url(id: &GifId) -> String {
    format!("https://giphy.com/gifs/{}", id.as_str())
}

/// What a message link to a GIF shows in place of the address.
pub(crate) const LABEL: &str = "GIF";

/// `text` as a chat line shows it with its GIF, and the GIF: the first GIPHY link made
/// [`LABEL`] (the GIF shows under the line); `text` unchanged and no GIF without one.
pub(crate) fn with_label(text: &str) -> (String, Option<GifId>) {
    match find(text) {
        Some((id, range)) => {
            let mut shown = String::with_capacity(text.len());
            shown.push_str(&text[..range.start]);
            shown.push_str(LABEL);
            shown.push_str(&text[range.end..]);
            (shown, Some(id))
        }
        None => (text.to_owned(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "3o7TKSjRrfIPjeiVyM";

    fn id(value: &str) -> Option<GifId> {
        GifId::new(value)
    }

    #[test]
    fn every_shape_giphy_uses_gives_its_id() {
        for url in [
            "https://giphy.com/gifs/the-office-michael-scott-3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM/",
            "https://www.giphy.com/gifs/funny-cat-3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/stickers/hello-wave-3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/embed/3o7TKSjRrfIPjeiVyM",
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/200w.webp",
            "https://media0.giphy.com/media/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://media4.giphy.com/media/3o7TKSjRrfIPjeiVyM/giphy-downsized.gif",
            "https://media2.giphy.com/media/v1.Y2lkPTc5MGI3NjExbWh5/3o7TKSjRrfIPjeiVyM/giphy.gif?cid=790b7611&ep=v1_gifs_search&rid=giphy.gif&ct=g",
            "https://i.giphy.com/3o7TKSjRrfIPjeiVyM.gif",
            "https://i.giphy.com/3o7TKSjRrfIPjeiVyM.webp",
            "https://i.giphy.com/media/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://i.giphy.com/media/v1.Y2lkPTc5MGI3NjEx/3o7TKSjRrfIPjeiVyM/200.gif",
            "https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM?utm_source=share#top",
            "HTTPS://GIPHY.COM/gifs/3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM.",
            "https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM!)",
        ] {
            assert_eq!(parse(url), id(ID), "{url}");
        }
    }

    #[test]
    fn http_is_taken_and_fetched_over_https() {
        let found = parse("http://giphy.com/gifs/3o7TKSjRrfIPjeiVyM").expect("an id");
        assert_eq!(
            media_url(&found, Rendition::Small),
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/200.gif"
        );
        assert_eq!(parse("http://i.giphy.com/3o7TKSjRrfIPjeiVyM.gif"), id(ID));
        assert_eq!(parse("ftp://giphy.com/gifs/3o7TKSjRrfIPjeiVyM"), None);
        assert_eq!(
            parse("giphy.com/gifs/3o7TKSjRrfIPjeiVyM"),
            None,
            "no scheme"
        );
    }

    #[test]
    fn lookalike_hosts_users_and_ports_are_refused() {
        for url in [
            "https://giphy.com.evil.example/gifs/3o7TKSjRrfIPjeiVyM",
            "https://evilgiphy.com/gifs/3o7TKSjRrfIPjeiVyM",
            "https://giphy.co/gifs/3o7TKSjRrfIPjeiVyM",
            "https://media5.giphy.com/media/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://media.giphy.com.evil.example/media/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://evil.example/media.giphy.com/media/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://giphy.com@evil.example/gifs/3o7TKSjRrfIPjeiVyM",
            "https://user@giphy.com/gifs/3o7TKSjRrfIPjeiVyM",
            "https://giphy.com:8443/gifs/3o7TKSjRrfIPjeiVyM",
            "https://gph.is/2bQwZbU",
            "https://tenor.com/view/3o7TKSjRrfIPjeiVyM",
        ] {
            assert_eq!(parse(url), None, "{url}");
        }
    }

    #[test]
    fn bad_ids_and_paths_are_refused() {
        for url in [
            "https://giphy.com/gifs/happy-birthday",
            "https://giphy.com/gifs/abc12",
            "https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM3o7TKSjRrfIPjeiVyM3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/gifs/3o7TKSj_RrfIPjeiVyM",
            "https://giphy.com/gifs/3o7TKSj%52rfIPjeiVyM",
            "https://giphy.com/gifs/../3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/gifs//3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/gifs/user/slug-3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/clips/funny-3o7TKSjRrfIPjeiVyM",
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM",
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/",
            "https://media.giphy.com/media/v2.token/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://media.giphy.com/media/v1.tok%en/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://media.giphy.com/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://i.giphy.com/3o7TKSjRrfIPjeiVyM.png",
            "https://i.giphy.com/3o7TKSjRrfIPjeiVyM",
            "https://giphy.com/gifs/3o7TKSjRrfIPjéiVyM",
        ] {
            assert_eq!(parse(url), None, "{url}");
        }
        assert!(id("ICOgUNjpvO0PC").is_some() && id("l0HlvtIPzPdt2usKs").is_some());
        assert!(id("abcdefgh").is_none(), "all small letters is a word");
    }

    #[test]
    fn the_address_fetched_is_rebuilt_on_the_media_host() {
        let found = id(ID).unwrap();
        let small = media_url(&found, Rendition::Small);
        let original = media_url(&found, Rendition::Original);
        assert_eq!(small, format!("https://media.giphy.com/media/{ID}/200.gif"));
        assert_eq!(
            original,
            format!("https://media.giphy.com/media/{ID}/giphy.gif")
        );
        let preview = media_url(&found, Rendition::Preview);
        assert_eq!(
            preview,
            format!("https://media.giphy.com/media/{ID}/200w_d.gif")
        );
        assert!(fetchable(&small) && fetchable(&original) && fetchable(&preview));
        assert_eq!(page_url(&found), format!("https://giphy.com/gifs/{ID}"));
        // Whatever was pasted, only those three are ever fetched.
        let pasted = "https://media2.giphy.com/media/v1.Y2lk/3o7TKSjRrfIPjeiVyM/giphy.gif?x=1";
        assert_eq!(media_url(&parse(pasted).unwrap(), Rendition::Small), small);
        for url in [
            pasted,
            "http://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/200.gif",
            "https://media.giphy.com.evil.example/media/3o7TKSjRrfIPjeiVyM/200.gif",
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/200.gif?x",
            "https://media.giphy.com/media/notanid/200.gif",
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/../200.gif",
        ] {
            assert!(!fetchable(url), "{url}");
        }
    }

    #[test]
    fn the_first_link_becomes_the_label() {
        let text = "look https://giphy.com/gifs/cat-3o7TKSjRrfIPjeiVyM! and \
                    https://giphy.com/gifs/ICOgUNjpvO0PC";
        let (shown, gif) = with_label(text);
        assert_eq!(gif, id(ID));
        assert_eq!(shown, "look GIF! and https://giphy.com/gifs/ICOgUNjpvO0PC");
        let (shown, gif) = with_label("no link here: https://example.com/a.gif");
        assert_eq!(gif, None);
        assert_eq!(shown, "no link here: https://example.com/a.gif");
        assert_eq!(
            find("https://i.giphy.com/3o7TKSjRrfIPjeiVyM.gif"),
            Some((id(ID).unwrap(), 0..42))
        );
    }

    #[test]
    fn the_chats_rules_let_giphy_links_through() {
        for url in [
            "https://giphy.com/gifs/the-office-michael-scott-3o7TKSjRrfIPjeiVyM",
            "https://media.giphy.com/media/3o7TKSjRrfIPjeiVyM/giphy.gif",
            "https://i.giphy.com/3o7TKSjRrfIPjeiVyM.webp",
            "https://media2.giphy.com/media/v1.Y2lkPTc5MGI3NjExbWh5/3o7TKSjRrfIPjeiVyM/giphy.gif?cid=790b7611&ct=g",
        ] {
            assert_eq!(sjk_identity::chat::check(url).as_deref(), Ok(url), "{url}");
            assert_eq!(sjk_identity::chat::for_display(url), url);
        }
        // A share link with all its tracking words is longer than a message may be.
        let long = format!(
            "https://media4.giphy.com/media/v1.{}/3o7TKSjRrfIPjeiVyM/giphy.gif?cid={}&ep=v1_gifs_search&rid=giphy.gif&ct=g",
            "Y2lkPTc5MGI3NjExNHh5dGZ6a3B3eGx2bWl5bXJ4eHNtYzN0eGVwZ3N2a3BkcWVlY2Z0dyZlcD12MV9pbnRlcm5hbF9naWZfYnlfaWQmY3Q9Zw",
            "790b7611",
        );
        assert_eq!(
            sjk_identity::chat::check(&long),
            Err(sjk_identity::chat::ChatError::Length)
        );
    }
}
