//! GIPHY's search for the GIF picker (`docs/hub-chat.md`, "The GIF picker"): the words
//! typed (or nothing, for GIPHY's trending GIFs), asked of GIPHY's API by this PC, as
//! GIPHY's terms ask (requests come from the client, never through a proxy such as the
//! hub). Only the GIFs' ids are read from the answer: what is fetched for each is
//! rebuilt from its id on GIPHY's media host ([`super::link::media_url`]).
//!
//! The key is the player's own (`cl_giphyKey`): SJK has none of its own (Sol's decision,
//! 12/10/2026), as GIPHY allows a new key 100 requests an hour, too few for every SJK
//! player to share. The picker's key card says how to get one ([`KEY_PAGE`]). A search
//! is asked only once the player stops typing (the picker waits [`DEBOUNCE`]), a page of
//! [`PAGE_SIZE`] at a time and no more than [`MOST`] for one search, and answers are
//! kept [`KEEP_FOR`] ([`Searcher`]); one request is on its way at a time. A refusal is
//! kept too, so a spent key is not asked again at once.

use super::GifId;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// `cl_giphyKey`: the player's own GIPHY API key, which search needs.
pub(crate) const KEY_CVAR: &str = "cl_giphyKey";
/// GIPHY's developer dashboard, where a player makes their key.
pub(crate) const KEY_PAGE: &str = "https://developers.giphy.com/dashboard/";
/// The most characters a key may have (GIPHY's are 32 letters and digits).
pub(crate) const KEY_MAX: usize = 64;
/// GIPHY's API host, and nothing else.
pub(crate) const API_HOST: &str = "api.giphy.com";
/// GIFs asked for at a time.
pub(crate) const PAGE_SIZE: usize = 25;
/// The most GIFs one search shows.
pub(crate) const MOST: usize = 100;
/// The rating GIPHY filters by: "pg-13", none of its "r" GIFs.
pub(crate) const RATING: &str = "pg-13";
/// The longest search GIPHY takes, in characters.
pub(crate) const QUERY_MAX: usize = 50;
/// How long the picker waits after the last key before it searches.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(700);
/// How long an answer is kept.
pub(crate) const KEEP_FOR: Duration = Duration::from_secs(15 * 60);
/// How long a refusal is kept: GIPHY's limit is counted by the hour.
const FAILED_FOR: Duration = Duration::from_secs(60);
/// Answers kept.
const KEPT: usize = 32;
/// How long one request may take.
const TIMEOUT: Duration = Duration::from_secs(10);
/// The most bytes read of an answer (a page of 25 is about 60 KiB).
const MAX_BYTES: u64 = 1 << 20;

/// The key searches use: the player's own (`own`, `cl_giphyKey`), trimmed; `None` when
/// it is not set.
pub(crate) fn key(own: &str) -> Option<&str> {
    let own = own.trim();
    (!own.is_empty()).then_some(own)
}

/// Whether `typed` looks like a GIPHY key, so a paste of something else is caught
/// before it is saved: 16 to [`KEY_MAX`] letters and digits.
pub(crate) fn looks_like_key(typed: &str) -> bool {
    let typed = typed.trim();
    (16..=KEY_MAX).contains(&typed.len()) && typed.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// A search as it is asked: the words, tidied (lower case, spaces made one, at most
/// [`QUERY_MAX`] characters), empty for GIPHY's trending GIFs.
pub(crate) fn tidy(words: &str) -> String {
    words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .take(QUERY_MAX)
        .collect()
}

/// One page of a search.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Ask {
    words: String,
    offset: usize,
}

/// The ids a page found and how many GIPHY says the search finds in all.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Page {
    pub(crate) ids: Vec<GifId>,
    pub(crate) total: usize,
}

/// Why a search could not be had.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Failure {
    /// `cl_giphyKey` is not set.
    NoKey,
    /// GIPHY refused the key (401, 403).
    BadKey,
    /// The key's requests for the hour are spent (429).
    Limited,
    /// Another status.
    Status(u16),
    /// Not reached, or the answer broke off.
    Network(String),
    /// Not GIPHY's JSON.
    Malformed,
}

impl Failure {
    /// What the picker says.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::NoKey => "GIF search needs your own GIPHY key: it's free.".to_owned(),
            Self::BadKey => "GIPHY refused your key: check it, or make a new one.".to_owned(),
            Self::Limited => {
                "Your GIPHY key's searches for this hour are spent: try again later.".to_owned()
            }
            Self::Status(status) => format!("GIPHY answered {status}"),
            Self::Network(error) => format!("GIPHY not reached: {error}"),
            Self::Malformed => "GIPHY's answer could not be read".to_owned(),
        }
    }
}

/// The address asked for `ask` with `key`: GIPHY's trending GIFs for no words, else its
/// search, `PAGE_SIZE` from `offset`, rated [`RATING`] at most.
fn url(key: &str, ask: &Ask) -> String {
    let mut url = if ask.words.is_empty() {
        format!("https://{API_HOST}/v1/gifs/trending?api_key=")
    } else {
        format!("https://{API_HOST}/v1/gifs/search?api_key=")
    };
    encode_into(&mut url, key);
    if !ask.words.is_empty() {
        url.push_str("&q=");
        encode_into(&mut url, &ask.words);
    }
    url.push_str(&format!(
        "&limit={PAGE_SIZE}&offset={}&rating={RATING}",
        ask.offset
    ));
    url
}

/// `value` percent-encoded onto `url`: letters, digits and `-._~` as they are, every
/// other byte as `%XX`.
fn encode_into(url: &mut String, value: &str) {
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
}

/// GIPHY's answer, the parts read.
#[derive(serde::Deserialize)]
struct Answer {
    #[serde(default)]
    data: Vec<Item>,
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(serde::Deserialize)]
struct Item {
    #[serde(default)]
    id: String,
}

#[derive(serde::Deserialize)]
struct Pagination {
    #[serde(default)]
    total_count: usize,
}

/// The page an answer's `body` holds: its GIFs' ids (those that are not ids left out,
/// each once) and the search's total, at most [`MOST`].
fn parse(body: &[u8]) -> Result<Page, Failure> {
    let answer: Answer = serde_json::from_slice(body).map_err(|_| Failure::Malformed)?;
    let mut ids: Vec<GifId> = Vec::with_capacity(answer.data.len());
    for item in &answer.data {
        if let Some(id) = GifId::new(&item.id)
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    let total = answer
        .pagination
        .map_or(ids.len(), |pagination| pagination.total_count)
        .min(MOST);
    Ok(Page { ids, total })
}

/// One request: the status and, for a 200, the body.
fn get(agent: &ureq::Agent, url: &str) -> Result<Page, Failure> {
    let mut response = agent
        .get(url)
        .header("Accept", "application/json")
        .call()
        .map_err(|error| Failure::Network(error.to_string()))?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(Failure::BadKey),
        429 => return Err(Failure::Limited),
        status => return Err(Failure::Status(status)),
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|error| Failure::Network(error.to_string()))?;
    parse(&body)
}

/// The worker: one request at a time, until the picker is gone. The key is never
/// logged (it travels in the address).
fn work(inbox: &Receiver<(String, Ask)>, outbox: &Sender<(Ask, Result<Page, Failure>)>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(format!("SJK/{}", crate::build_info::VERSION))
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    while let Ok((key, ask)) = inbox.recv() {
        let answer = get(&agent, &url(&key, &ask));
        if let Err(failure) = &answer {
            crate::log::progress(format_args!("GIF search: {failure:?}"));
        }
        if outbox.send((ask, answer)).is_err() {
            return;
        }
    }
}

/// What a search shows so far.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Results {
    /// The GIFs found, page after page.
    pub(crate) ids: Vec<GifId>,
    /// GIPHY's count for the search (at most [`MOST`]), once a page came.
    pub(crate) total: Option<usize>,
    /// A page is on its way.
    pub(crate) loading: bool,
    /// Why the next page could not be had.
    pub(crate) failure: Option<Failure>,
}

/// An answer kept: the page, or why it could not be had, and when.
struct Kept {
    ask: Ask,
    at: Instant,
    answer: Result<Page, Failure>,
}

/// The picker's searches: GIPHY's answers kept, the one request on its way and the
/// worker that asks.
#[derive(Default)]
pub(crate) struct Searcher {
    kept: Vec<Kept>,
    pending: Option<Ask>,
    requests: Option<Sender<(String, Ask)>>,
    answers: Option<Receiver<(Ask, Result<Page, Failure>)>>,
    /// Counts answers taken, so the picker knows when its tiles changed.
    revision: u64,
    /// The key the last search was asked with: a new one forgets the refusals kept.
    key: Option<String>,
}

impl Searcher {
    /// Counts the answers taken.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Take the worker's answers.
    pub(crate) fn poll(&mut self) {
        let Some(answers) = &self.answers else {
            return;
        };
        let taken: Vec<_> = answers.try_iter().collect();
        for (ask, answer) in taken {
            if self.pending.as_ref() == Some(&ask) {
                self.pending = None;
            }
            self.keep(ask, answer);
        }
    }

    fn keep(&mut self, ask: Ask, answer: Result<Page, Failure>) {
        self.kept.retain(|kept| kept.ask != ask);
        if self.kept.len() == KEPT {
            self.kept.remove(0);
        }
        self.kept.push(Kept {
            ask,
            at: Instant::now(),
            answer,
        });
        self.revision += 1;
    }

    /// The kept answer to `ask`, while it is fresh.
    fn kept(&self, ask: &Ask) -> Option<&Result<Page, Failure>> {
        let kept = self.kept.iter().find(|kept| kept.ask == *ask)?;
        let fresh = match kept.answer {
            Ok(_) => KEEP_FOR,
            Err(_) => FAILED_FOR,
        };
        (kept.at.elapsed() < fresh).then_some(&kept.answer)
    }

    /// The first `pages` pages of the search for `words` ([`tidy`]), with `key` (None:
    /// not set), asking GIPHY for the first page not had when nothing is on
    /// its way.
    pub(crate) fn results(&mut self, key: Option<&str>, words: &str, pages: usize) -> Results {
        if self.key.as_deref() != key {
            self.key = key.map(str::to_owned);
            let before = self.kept.len();
            self.kept.retain(|kept| kept.answer.is_ok());
            if self.kept.len() != before {
                self.revision += 1;
            }
        }
        let mut results = Results::default();
        for page in 0..pages.max(1) {
            let ask = Ask {
                words: words.to_owned(),
                offset: page * PAGE_SIZE,
            };
            if page > 0 && results.total.is_some_and(|total| ask.offset >= total) {
                break;
            }
            match self.kept(&ask) {
                Some(Ok(found)) => {
                    results.total = Some(found.total);
                    for id in &found.ids {
                        if !results.ids.contains(id) {
                            results.ids.push(id.clone());
                        }
                    }
                    if found.ids.is_empty() {
                        break;
                    }
                }
                Some(Err(failure)) => {
                    results.failure = Some(failure.clone());
                    break;
                }
                None => {
                    match key {
                        None => results.failure = Some(Failure::NoKey),
                        Some(key) => {
                            results.loading = true;
                            if self.pending.is_none() {
                                self.ask(key, ask);
                            }
                        }
                    }
                    break;
                }
            }
        }
        results
    }

    /// Ask the worker for `ask`, starting it the first time.
    fn ask(&mut self, key: &str, ask: Ask) {
        if cfg!(test) && self.requests.is_none() {
            // The tests never reach GIPHY.
            self.pending = Some(ask);
            return;
        }
        if self.requests.is_none() {
            let (requests, inbox) = channel();
            let (outbox, answers) = channel();
            let spawned = std::thread::Builder::new()
                .name("sjk-gif-search".to_owned())
                .spawn(move || work(&inbox, &outbox));
            if let Err(error) = spawned {
                self.keep(ask, Err(Failure::Network(error.to_string())));
                return;
            }
            self.requests = Some(requests);
            self.answers = Some(answers);
        }
        let sent = self
            .requests
            .as_ref()
            .is_some_and(|requests| requests.send((key.to_owned(), ask.clone())).is_ok());
        if sent {
            self.pending = Some(ask);
        } else {
            self.keep(ask, Err(Failure::Network("the search stopped".to_owned())));
        }
    }

    /// Answer the request on its way with `answer`, as the worker would, for the tests.
    #[cfg(test)]
    pub(crate) fn answer_for_test(&mut self, answer: Result<Page, Failure>) {
        if let Some(ask) = self.pending.take() {
            self.keep(ask, answer);
        }
    }

    /// The words and offset of the request on its way, for the tests.
    #[cfg(test)]
    pub(crate) fn pending_for_test(&self) -> Option<(&str, usize)> {
        self.pending
            .as_ref()
            .map(|ask| (ask.words.as_str(), ask.offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: usize) -> GifId {
        GifId::new(&format!("Res{n:04}xyz")).unwrap()
    }

    fn page(from: usize, count: usize, total: usize) -> Page {
        Page {
            ids: (from..from + count).map(id).collect(),
            total,
        }
    }

    #[test]
    fn only_the_players_own_key_is_used() {
        assert_eq!(key("  mykey "), Some("mykey"));
        assert_eq!(key("  "), None, "SJK has no key of its own");
        assert!(looks_like_key(" 0123456789abcdefABCDEF0123456789 "));
        assert!(!looks_like_key("short"));
        assert!(!looks_like_key("https://developers.giphy.com/dashboard/"));
        assert!(!looks_like_key(&"a".repeat(KEY_MAX + 1)));
    }

    #[test]
    fn words_are_tidied_and_cut_to_giphys_length() {
        assert_eq!(tidy("  Happy   Birthday "), "happy birthday");
        assert_eq!(tidy(""), "");
        assert_eq!(tidy(&"a".repeat(80)).len(), QUERY_MAX);
    }

    #[test]
    fn the_address_is_giphys_search_or_trending_with_the_words_encoded() {
        let search = url(
            "k3y",
            &Ask {
                words: "lightsaber duel & co".to_owned(),
                offset: 25,
            },
        );
        assert_eq!(
            search,
            "https://api.giphy.com/v1/gifs/search?api_key=k3y&q=lightsaber%20duel%20%26%20co&limit=25&offset=25&rating=pg-13"
        );
        let trending = url(
            "k3y",
            &Ask {
                words: String::new(),
                offset: 0,
            },
        );
        assert_eq!(
            trending,
            "https://api.giphy.com/v1/gifs/trending?api_key=k3y&limit=25&offset=0&rating=pg-13"
        );
        let mut encoded = String::new();
        encode_into(&mut encoded, "é?");
        assert_eq!(encoded, "%C3%A9%3F");
    }

    #[test]
    fn only_the_ids_are_read_from_giphys_answer() {
        let body =
            br#"{"data":[{"type":"gif","id":"3o7TKSjRrfIPjeiVyM","url":"https://evil.example"},
            {"id":"birthday"},{"id":"3o7TKSjRrfIPjeiVyM"},{"id":"l0HlvtIPzPdt2usKs"}],
            "pagination":{"total_count":4021,"count":4,"offset":0},"meta":{"status":200}}"#;
        let found = parse(body).unwrap();
        assert_eq!(
            found.ids,
            [
                GifId::new("3o7TKSjRrfIPjeiVyM").unwrap(),
                GifId::new("l0HlvtIPzPdt2usKs").unwrap()
            ]
        );
        assert_eq!(found.total, MOST, "a search shows at most MOST");
        assert_eq!(parse(b"<html>"), Err(Failure::Malformed));
        assert_eq!(parse(br#"{"data":[]}"#), Ok(Page::default()));
    }

    #[test]
    fn one_request_at_a_time_and_answers_are_kept() {
        let mut searcher = Searcher::default();
        let first = searcher.results(Some("k"), "cats", 1);
        assert!(first.loading && first.ids.is_empty());
        assert_eq!(searcher.pending_for_test(), Some(("cats", 0)));
        // Asked again while on its way: nothing more is sent.
        let _ = searcher.results(Some("k"), "dogs", 1);
        assert_eq!(searcher.pending_for_test(), Some(("cats", 0)));
        searcher.answer_for_test(Ok(page(0, PAGE_SIZE, 60)));
        let shown = searcher.results(Some("k"), "cats", 1);
        assert_eq!(shown.ids.len(), PAGE_SIZE);
        assert_eq!((shown.total, shown.loading), (Some(60), false));
        assert_eq!(searcher.pending_for_test(), None, "kept: not asked again");
        // A second page is asked from where the first ended.
        let more = searcher.results(Some("k"), "cats", 2);
        assert!(more.loading && more.ids.len() == PAGE_SIZE);
        assert_eq!(searcher.pending_for_test(), Some(("cats", PAGE_SIZE)));
        searcher.answer_for_test(Ok(page(PAGE_SIZE, PAGE_SIZE, 60)));
        let both = searcher.results(Some("k"), "cats", 2);
        assert_eq!(both.ids.len(), 2 * PAGE_SIZE);
        // No page past the total is asked.
        let _ = searcher.results(Some("k"), "cats", 3);
        let _ = searcher.results(Some("k"), "cats", 3);
        searcher.answer_for_test(Ok(page(50, 10, 60)));
        let all = searcher.results(Some("k"), "cats", 4);
        assert_eq!((all.ids.len(), all.loading), (60, false));
        assert_eq!(searcher.pending_for_test(), None);
    }

    #[test]
    fn a_refusal_is_kept_and_said_without_a_key_nothing_is_asked() {
        let mut searcher = Searcher::default();
        let shown = searcher.results(None, "cats", 1);
        assert_eq!(shown.failure, Some(Failure::NoKey));
        assert_eq!(searcher.pending_for_test(), None);
        let _ = searcher.results(Some("k"), "cats", 1);
        searcher.answer_for_test(Err(Failure::Limited));
        let shown = searcher.results(Some("k"), "cats", 1);
        assert_eq!(shown.failure, Some(Failure::Limited));
        assert_eq!(searcher.pending_for_test(), None, "not asked again at once");
        assert!(Failure::NoKey.message().contains("own GIPHY key"));
        // A new key forgets the refusal: it is asked at once.
        let _ = searcher.results(Some("k2"), "cats", 1);
        assert_eq!(searcher.pending_for_test(), Some(("cats", 0)));
    }
}
