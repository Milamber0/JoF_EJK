//! Talking to the hub: the [`Hub`] operations and their HTTPS implementation.

use crate::assets::{PACK_MAX, Pack};
use crate::crash::CrashReport;
use crate::keys::{Identity, random_bytes};
use crate::logs::{
    CrashDetail, CrashGroups, CrashSummaries, LogRequest, LogRequestRow, LogRequestRows,
    LogRequests, LogUpload,
};
use crate::report::{BugReport, PlayerReport, WorldNote};
use crate::staff::{StaffAnswer, StaffQuery, StaffRequest};
use crate::vitals::Sharing;
use crate::wire::{
    Achievement, Achievements, Feed, HolocronState, Look, OpenAnswer, Players, Presence, Profile,
    SharedVitals, authorization,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Longest the hub may take to answer.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Largest answer read, in bytes.
const ANSWER_MAX: u64 = 256 * 1024;
/// Largest answer to a staff query: one request's logs, up to 400 000 characters of
/// any script, escaped in JSON.
const STAFF_ANSWER_MAX: u64 = 4 * 1024 * 1024;
/// Longest a log upload may take (up to half a megabyte on a slow line).
const LOGS_TIMEOUT: Duration = Duration::from_secs(60);
/// Longest a pack's download may take (up to [`PACK_MAX`] bytes).
const ASSET_TIMEOUT: Duration = Duration::from_secs(120);

/// Why a hub request failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HubError {
    /// The hub could not be reached.
    Network(String),
    /// The hub answered with an error.
    Rejected {
        /// HTTP status.
        status: u16,
        /// The hub's machine-readable code (`name_taken`, `slot_taken`, ...).
        code: String,
        /// The hub's text.
        message: String,
    },
    /// The answer was not what `PROTOCOL.md` describes.
    Protocol(String),
}

impl std::fmt::Display for HubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(why) => write!(f, "cannot reach the hub: {why}"),
            Self::Rejected { message, code, .. } => {
                write!(f, "the hub refused it ({code}): {message}")
            }
            Self::Protocol(why) => write!(f, "unexpected answer from the hub: {why}"),
        }
    }
}

impl std::error::Error for HubError {}

/// What the client asks of a hub (`PROTOCOL.md`, "Endpoints").
pub trait Hub: Send {
    /// Register the identity's key, or fetch its profile if it is known, telling
    /// the hub the in-game `name` the player wears, which it keeps in the key's
    /// name history.
    fn register(&mut self, identity: &Identity, name: Option<&str>) -> Result<Profile, HubError>;
    /// Set the identity's bio (the name is the one worn in game).
    fn set_bio(&mut self, identity: &Identity, bio: &str) -> Result<Profile, HubError>;
    /// Send the counts the client keeps for the achievements it counts (`id` to
    /// count); the hub answers with the key's achievements as it now holds them.
    fn set_achievements(
        &mut self,
        _identity: &Identity,
        _progress: &BTreeMap<String, u64>,
    ) -> Result<Vec<Achievement>, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send achievements".to_owned(),
        ))
    }
    /// A staff request signed by the identity (`PROTOCOL.md`, "Staff"): the players a
    /// search found, or the changed key's profile.
    fn staff(
        &mut self,
        _identity: &Identity,
        _request: &StaffRequest,
    ) -> Result<Vec<Profile>, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send staff requests".to_owned(),
        ))
    }
    /// A staff query signed by the identity (`PROTOCOL.md`, "Log requests"): asking a
    /// player for their logs, reading the requests and their logs, and crash reports.
    fn staff_query(
        &mut self,
        _identity: &Identity,
        _query: &StaffQuery,
    ) -> Result<StaffAnswer, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send staff queries".to_owned(),
        ))
    }
    /// The SJK team's open requests for the identity's logs, oldest first.
    fn log_requests(&mut self, _identity: &Identity) -> Result<Vec<LogRequest>, HubError> {
        Err(HubError::Protocol(
            "this hub client does not read log requests".to_owned(),
        ))
    }
    /// Send `upload` (already [`LogUpload::fitted`]) for request `id`.
    fn send_logs(
        &mut self,
        _identity: &Identity,
        _id: i64,
        _upload: &LogUpload,
    ) -> Result<(), HubError> {
        Err(HubError::Protocol(
            "this hub client does not send logs".to_owned(),
        ))
    }
    /// Decline request `id`.
    fn decline_logs(&mut self, _identity: &Identity, _id: i64) -> Result<(), HubError> {
        Err(HubError::Protocol(
            "this hub client does not send logs".to_owned(),
        ))
    }
    /// Send an SJK chat message (already checked by [`crate::chat::check`]) signed by
    /// the identity, with the in-game `name` worn (may be empty); the hub answers with
    /// its id.
    fn chat(&mut self, _identity: &Identity, _text: &str, _name: &str) -> Result<u64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send chat".to_owned(),
        ))
    }
    /// Play `emote` for the slot the identity claims on `server`, in step with the
    /// player in slot `sync` when it is set.
    fn emote(
        &mut self,
        _identity: &Identity,
        _server: &str,
        _emote: &str,
        _sync: Option<u8>,
    ) -> Result<u64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send emotes".to_owned(),
        ))
    }
    /// Mark `pos` (whole game units) for `scope` from the slot the identity claims on
    /// `server` (`PROTOCOL.md`, "Pings"); the hub answers with the ping's feed id.
    fn ping(
        &mut self,
        _identity: &Identity,
        _server: &str,
        _pos: [i32; 3],
        _scope: &str,
    ) -> Result<u64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send pings".to_owned(),
        ))
    }
    /// Tell the hub which player models the game has (`PROTOCOL.md`, "Models"); the
    /// hub answers how many names it kept.
    fn set_models(&mut self, _identity: &Identity, _models: &[String]) -> Result<u64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send models".to_owned(),
        ))
    }
    /// Whether the player claiming `slot` on `server` has `model` (`PROTOCOL.md`,
    /// "Models"): their claimed name and the answer, `None` when they sent no list.
    fn model_check(
        &mut self,
        _server: &str,
        _slot: u8,
        _model: &str,
    ) -> Result<(String, Option<bool>), HubError> {
        Err(HubError::Protocol(
            "this hub client does not check models".to_owned(),
        ))
    }
    /// Share (or stop sharing) the player's vitals on `server` and read the sharers
    /// there (`PROTOCOL.md`, "Vitals").
    fn vitals(
        &mut self,
        _identity: &Identity,
        _server: &str,
        _sharing: &Sharing,
    ) -> Result<Vec<SharedVitals>, HubError> {
        Err(HubError::Protocol(
            "this hub client does not share vitals".to_owned(),
        ))
    }
    /// Wear `look` for the claim the identity holds on `server` (`PROTOCOL.md`,
    /// "Looks"); the hub answers with the feed id of its event.
    fn look(&mut self, _identity: &Identity, _server: &str, _look: &Look) -> Result<u64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send looks".to_owned(),
        ))
    }
    /// What is new after `after` (chat, and the emotes and looks of `server`), waiting up to `wait`
    /// seconds at the hub for something to come.
    fn feed(
        &mut self,
        _identity: &Identity,
        _after: u64,
        _server: Option<&str>,
        _wait: u64,
    ) -> Result<Feed, HubError> {
        Err(HubError::Protocol(
            "this hub client does not read the feed".to_owned(),
        ))
    }
    /// Make `png` (already cropped and scaled, `crate::avatar`) the identity's picture;
    /// the hub answers with the key's profile, whose `avatar` is the new version.
    fn set_avatar(&mut self, _identity: &Identity, _png: &[u8]) -> Result<Profile, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send pictures".to_owned(),
        ))
    }
    /// Take the identity's picture down; the hub answers with the key's profile.
    fn remove_avatar(&mut self, _identity: &Identity) -> Result<Profile, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send pictures".to_owned(),
        ))
    }
    /// The PNG of `version` of `key_id`'s picture, as the hub serves it.
    fn avatar(&mut self, _key_id: &str, _version: &str) -> Result<Vec<u8>, HubError> {
        Err(HubError::Protocol(
            "this hub client does not read pictures".to_owned(),
        ))
    }
    /// The asset packs the hub serves (`PROTOCOL.md`, "Assets"), sorted by name.
    fn assets(&mut self, _identity: &Identity) -> Result<Vec<Pack>, HubError> {
        Err(HubError::Protocol(
            "this hub client does not read assets".to_owned(),
        ))
    }
    /// The bytes of asset pack `name`, at most [`PACK_MAX`].
    fn asset(&mut self, _identity: &Identity, _name: &str) -> Result<Vec<u8>, HubError> {
        Err(HubError::Protocol(
            "this hub client does not read assets".to_owned(),
        ))
    }
    /// Any player's public profile.
    fn profile(&mut self, key_id: &str) -> Result<Profile, HubError>;
    /// Say the identity's player is in `slot` of `server` as `name`. `active` says the
    /// player is playing and not idle (`PROTOCOL.md`, "Holocrons"): the hub counts the
    /// time since the last claim toward the key's next holocron only then. A hub
    /// without holocrons ignores it.
    fn claim(
        &mut self,
        identity: &Identity,
        server: &str,
        slot: u8,
        name: &str,
        active: bool,
    ) -> Result<(), HubError>;
    /// How far the identity's key is from its next holocron (`GET /v1/holocrons`).
    fn holocrons(&mut self, _identity: &Identity) -> Result<HolocronState, HubError> {
        Err(HubError::Protocol(
            "this hub client does not read holocrons".to_owned(),
        ))
    }
    /// Open the identity's holocrons `ids` (1 to [`crate::wire::OPEN_MAX`], its own, not
    /// opened yet), all or none (`POST /v1/holocrons/open`): each brings a sealed card and
    /// credits. The hub answers what each brought and the person's profile after.
    fn open_holocrons(
        &mut self,
        _identity: &Identity,
        _ids: &[u64],
    ) -> Result<OpenAnswer, HubError> {
        Err(HubError::Protocol(
            "this hub client does not open holocrons".to_owned(),
        ))
    }

    /// Reveal the identity's sealed card copies whose rarity has cards in the hub's
    /// catalogue (`POST /v1/cards/reveal`). The hub answers what each became and the
    /// person's profile after.
    fn reveal_cards(
        &mut self,
        _identity: &Identity,
    ) -> Result<crate::wire::RevealAnswer, HubError> {
        Err(HubError::Protocol(
            "this hub client does not reveal cards".to_owned(),
        ))
    }
    /// Withdraw the identity's claim on `server`.
    fn release(&mut self, identity: &Identity, server: &str) -> Result<(), HubError>;
    /// The live claims on `server`.
    fn presence(&mut self, server: &str) -> Result<Vec<Presence>, HubError>;
    /// Send a world note signed by the identity; the hub answers with its number.
    fn note(&mut self, _identity: &Identity, _note: &WorldNote) -> Result<i64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send notes".to_owned(),
        ))
    }
    /// Attach a JPEG to note `id`, signed by the identity that sent the note.
    fn note_image(&mut self, _identity: &Identity, _id: i64, _jpeg: &[u8]) -> Result<(), HubError> {
        Err(HubError::Protocol(
            "this hub client does not send pictures".to_owned(),
        ))
    }
    /// Send a bug report signed by the identity; the hub answers with its number.
    fn report(&mut self, _identity: &Identity, _report: &BugReport) -> Result<i64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send reports".to_owned(),
        ))
    }
    /// Send anonymous settings usage, unsigned (`PROTOCOL.md`, "Settings usage"):
    /// `body` is [`crate::usage::body`]'s. Answers how many events the hub kept.
    fn settings_usage(&mut self, _body: &Value) -> Result<u64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send settings usage".to_owned(),
        ))
    }
    /// The donation goal (`PROTOCOL.md`, "Donations"), asked without a signature.
    fn donation_goal(&mut self) -> Result<crate::DonationGoal, HubError> {
        Err(HubError::Protocol(
            "this hub client does not read the donation goal".to_owned(),
        ))
    }
    /// Send a crash report (already [`CrashReport::fitted`]) signed by the identity;
    /// the hub answers with its number.
    fn crash(&mut self, _identity: &Identity, _report: &CrashReport) -> Result<i64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send crash reports".to_owned(),
        ))
    }
    /// Send a report about another player, signed by the identity (a verified key);
    /// the hub answers with its number.
    fn player_report(
        &mut self,
        _identity: &Identity,
        _report: &PlayerReport,
    ) -> Result<i64, HubError> {
        Err(HubError::Protocol(
            "this hub client does not send player reports".to_owned(),
        ))
    }
}

/// Whether `url` may be used as the hub's address: `https://host[:port]`, or
/// plain `http://` only for this machine (local testing). No path: requests are
/// signed with their path, which must be the one the hub sees.
pub fn valid_base_url(url: &str) -> bool {
    let (secure, authority) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest)
    } else {
        return false;
    };
    let authority = authority.strip_suffix('/').unwrap_or(authority);
    if authority.is_empty() || authority.contains(['/', '?', '#', '@', ' ']) {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    !host.is_empty() && (secure || matches!(host, "localhost" | "127.0.0.1" | "::1"))
}

/// A JSON answer of `status`, read up to [`ANSWER_MAX`].
fn read_json(
    response: &mut ureq::http::Response<ureq::Body>,
    status: u16,
) -> Result<Value, HubError> {
    read_json_up_to(response, status, ANSWER_MAX)
}

/// [`read_json`] of an answer of at most `limit` bytes.
fn read_json_up_to(
    response: &mut ureq::http::Response<ureq::Body>,
    status: u16,
    limit: u64,
) -> Result<Value, HubError> {
    let text = response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map_err(|error| HubError::Network(error.to_string()))?;
    serde_json::from_str(&text)
        .map_err(|_| HubError::Protocol(format!("status {status}, not JSON")))
}

/// The error after the hub refused the timestamp twice.
fn clock_disagrees() -> HubError {
    HubError::Protocol("the hub's clock keeps disagreeing".to_owned())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// The hub over HTTPS.
pub struct HttpHub {
    base: String,
    agent: ureq::Agent,
    /// Added to the local clock to get the hub's, learned from a `clock` refusal.
    clock_offset: i64,
}

impl HttpHub {
    /// A client for the hub at `base_url` (`https://hub.example`, no trailing slash needed).
    pub fn new(base_url: &str, user_agent: &str) -> Result<Self, HubError> {
        Self::with_timeout(base_url, user_agent, TIMEOUT)
    }

    /// [`HttpHub::new`] whose requests may take `timeout` (the feed's long poll).
    pub fn with_timeout(
        base_url: &str,
        user_agent: &str,
        timeout: Duration,
    ) -> Result<Self, HubError> {
        let base = base_url.trim().trim_end_matches('/').to_owned();
        if !valid_base_url(&base) {
            return Err(HubError::Network(
                "the hub address must start with https://".to_owned(),
            ));
        }
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .user_agent(user_agent)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self {
            base,
            agent,
            clock_offset: 0,
        })
    }

    /// One request, signed when `signer` is given. Retries once with the hub's
    /// clock when it refuses the timestamp.
    fn send(
        &mut self,
        signer: Option<&Identity>,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, HubError> {
        let body = body.map(|value| value.to_string()).unwrap_or_default();
        self.send_bytes(signer, method, path, body.as_bytes(), "application/json")
    }

    /// [`HttpHub::send`] with a raw body of `content_type`.
    fn send_bytes(
        &mut self,
        signer: Option<&Identity>,
        method: &str,
        path: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<Value, HubError> {
        self.send_with(signer, method, path, body, content_type, ANSWER_MAX, None)
    }

    /// [`HttpHub::send_bytes`] reading an answer of up to `limit` bytes, with its own
    /// `timeout` when given.
    #[allow(clippy::too_many_arguments)]
    fn send_with(
        &mut self,
        signer: Option<&Identity>,
        method: &str,
        path: &str,
        body: &[u8],
        content_type: &str,
        limit: u64,
        timeout: Option<Duration>,
    ) -> Result<Value, HubError> {
        for attempt in 0..2 {
            let url = format!("{}{path}", self.base);
            let header = signer
                .map(|identity| self.authorization(identity, method, path, body))
                .transpose()?;
            let mut response =
                self.call(method, &url, header.as_deref(), body, content_type, timeout)?;
            let status = response.status().as_u16();
            let value = read_json_up_to(&mut response, status, limit)?;
            if status < 400 {
                return Ok(value);
            }
            if let Some(refusal) = self.refusal(status, &value, attempt == 0) {
                return Err(refusal);
            }
        }
        Err(clock_disagrees())
    }

    /// The `Authorization` header of a request signed by `identity` now, by the hub's
    /// clock as far as it is known.
    fn authorization(
        &self,
        identity: &Identity,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<String, HubError> {
        let nonce = random_bytes::<12>().map_err(|error| HubError::Network(error.to_string()))?;
        Ok(authorization(
            identity,
            method,
            path,
            body,
            unix_now() + self.clock_offset,
            nonce,
        ))
    }

    /// An unsigned `GET` of `path` whose answer is bytes, at most `limit` of them; an
    /// error answer is the hub's JSON, as for [`HttpHub::send`].
    fn fetch(&mut self, path: &str, limit: u64) -> Result<Vec<u8>, HubError> {
        let url = format!("{}{path}", self.base);
        let mut response = self
            .agent
            .get(&url)
            .call()
            .map_err(|error| HubError::Network(error.to_string()))?;
        let status = response.status().as_u16();
        if status < 400 {
            return response
                .body_mut()
                .with_config()
                .limit(limit)
                .read_to_vec()
                .map_err(|error| HubError::Network(error.to_string()));
        }
        let value = read_json(&mut response, status)?;
        Err(self
            .refusal(status, &value, false)
            .unwrap_or_else(clock_disagrees))
    }

    /// The error of an answer of `status` 400 or more carrying `value`; `None` when the
    /// hub refused the timestamp and `may_retry`: its clock is learned and the request
    /// should go again.
    fn refusal(&mut self, status: u16, value: &Value, may_retry: bool) -> Option<HubError> {
        let code = value["error"].as_str().unwrap_or("error").to_owned();
        if code == "clock"
            && may_retry
            && let Some(server_time) = value["server_time"].as_i64()
        {
            self.clock_offset = server_time - unix_now();
            return None;
        }
        Some(HubError::Rejected {
            status,
            code,
            message: value["message"].as_str().unwrap_or_default().to_owned(),
        })
    }

    /// A signed `GET` of raw bytes, at most `limit` of them, with its own `timeout`
    /// (an asset pack). An error answer is read as [`HttpHub::send`] reads it,
    /// retrying once with the hub's clock.
    fn get_bytes(
        &mut self,
        identity: &Identity,
        path: &str,
        limit: u64,
        timeout: Duration,
    ) -> Result<Vec<u8>, HubError> {
        for attempt in 0..2 {
            let url = format!("{}{path}", self.base);
            let header = self.authorization(identity, "GET", path, b"")?;
            let mut response = self
                .agent
                .get(&url)
                .header("Authorization", header.as_str())
                .config()
                .timeout_global(Some(timeout))
                .build()
                .call()
                .map_err(|error| HubError::Network(error.to_string()))?;
            let status = response.status().as_u16();
            if status < 400 {
                // One byte over the limit is read, so a longer answer is an error.
                return match response
                    .body_mut()
                    .with_config()
                    .limit(limit + 1)
                    .read_to_vec()
                {
                    Ok(bytes) if bytes.len() as u64 <= limit => Ok(bytes),
                    Ok(_) | Err(ureq::Error::BodyExceedsLimit(_)) => {
                        Err(HubError::Protocol("pack too large".to_owned()))
                    }
                    Err(error) => Err(HubError::Network(error.to_string())),
                };
            }
            let value = read_json(&mut response, status)?;
            if let Some(refusal) = self.refusal(status, &value, attempt == 0) {
                return Err(refusal);
            }
        }
        Err(clock_disagrees())
    }

    fn call(
        &self,
        method: &str,
        url: &str,
        authorization: Option<&str>,
        body: &[u8],
        content_type: &str,
        timeout: Option<Duration>,
    ) -> Result<ureq::http::Response<ureq::Body>, HubError> {
        let network = |error: ureq::Error| HubError::Network(error.to_string());
        let with_header =
            |builder: ureq::RequestBuilder<ureq::typestate::WithBody>| match authorization {
                Some(value) => builder.header("Authorization", value),
                None => builder,
            };
        if let Some(timeout) = timeout {
            return with_header(self.agent.post(url))
                .header("Content-Type", content_type)
                .config()
                .timeout_global(Some(timeout))
                .build()
                .send(body)
                .map_err(network);
        }
        match method {
            "GET" => {
                let mut request = self.agent.get(url);
                if let Some(value) = authorization {
                    request = request.header("Authorization", value);
                }
                request.call().map_err(network)
            }
            "DELETE" => {
                let mut request = self.agent.delete(url).force_send_body();
                if let Some(value) = authorization {
                    request = request.header("Authorization", value);
                }
                request
                    .header("Content-Type", content_type)
                    .send(body)
                    .map_err(network)
            }
            "PUT" => with_header(self.agent.put(url))
                .header("Content-Type", content_type)
                .send(body)
                .map_err(network),
            _ => with_header(self.agent.post(url))
                .header("Content-Type", content_type)
                .send(body)
                .map_err(network),
        }
    }
}

/// A note's remaps and model (`PROTOCOL.md`, "World notes"), added to `body` only when
/// it has them: a hub from before them refuses the fields, and still takes a note
/// without them.
fn note_context(body: &mut Value, note: &WorldNote) {
    if !note.remapped.is_empty() {
        body["remapped"] = json!(note.remapped);
    }
    if !note.remaps.is_empty() {
        body["remaps"] = note
            .remaps
            .iter()
            .map(|remap| json!({"source": remap.source, "from": remap.from, "to": remap.to}))
            .collect();
    }
    if let Some(model) = &note.model {
        let mut value = json!({
            "path": model.path,
            "classname": model.classname,
            "shaders": model
                .shaders
                .iter()
                .map(|shader| json!({
                    "surface": shader.surface,
                    "shader": shader.shader,
                    "remapped": shader.remapped,
                }))
                .collect::<Vec<_>>(),
        });
        if let Some(entity) = model.entity {
            value["entity"] = json!(entity);
        }
        if let Some(origin) = model.origin {
            value["origin"] = json!(origin);
        }
        if !model.skin.is_empty() {
            value["skin"] = json!(model.skin);
        }
        body["model"] = value;
    }
}

/// `POST /v1/player-report`'s body: empty optional fields are left out.
fn player_report_body(report: &PlayerReport) -> Value {
    let mut body = json!({
        "category": report.category.code(),
        "text": report.text,
        "server": report.server,
        "slot": report.slot,
        "target_name": report.target_name,
        "map": report.map,
        "build": report.build,
    });
    for (key, value) in [
        ("target_key_id", &report.target_key_id),
        ("server_name", &report.server_name),
        ("name", &report.name),
    ] {
        if !value.is_empty() {
            body[key] = json!(value);
        }
    }
    if let Some(time) = report.level_time {
        body["level_time"] = json!(time);
    }
    body
}

/// A game server's address as a query value: digits and dots as they are, the rest
/// (`:`, an IPv6 address's brackets) percent-encoded.
fn encode_server(server: &str) -> String {
    server
        .bytes()
        .map(|byte| match byte {
            b'0'..=b'9' | b'.' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// A model name for a query: letters, digits, `_`, `-` and `.` as they are, the rest
/// percent-encoded.
fn encode_model(model: &str) -> String {
    model
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// `GET /v1/feed`'s path and query.
fn feed_path(after: u64, server: Option<&str>, wait: u64) -> String {
    let mut path = format!("/v1/feed?after={after}&wait={wait}");
    if let Some(server) = server {
        path.push_str("&server=");
        path.push_str(&encode_server(server));
    }
    path
}

/// `POST /v1/look`'s body: exactly the server and the look's fields. `saber_off` goes only
/// when it names an option (and with a blade skin): a hub from before 11/10/2026 refuses
/// the field, so a look with every part on stays as it was.
fn look_body(server: &str, look: &Look) -> Value {
    let mut body = json!({ "server": server, "saber": look.saber, "illuminate": look.illuminate });
    if !look.saber.is_empty() && !look.saber_off.is_empty() {
        body["saber_off"] = json!(look.saber_off);
    }
    body
}

/// `POST /v1/claim`'s body. `active` is sent only when true: the field is optional and
/// absent means false, so a hub from before holocrons never sees an unknown field.
fn claim_body(server: &str, slot: u8, name: &str, active: bool) -> Value {
    let mut body = json!({"server": server, "slot": slot, "name": name});
    if active {
        body["active"] = json!(true);
    }
    body
}

/// `POST /v1/holocrons/open`'s path and body.
fn open_call(ids: &[u64]) -> (&'static str, Value) {
    ("/v1/holocrons/open", json!({ "ids": ids }))
}

/// A staff request's path and body.
fn staff_call(request: &StaffRequest) -> (&'static str, Value) {
    match request {
        StaffRequest::Search(query) => ("/v1/staff/search", json!({ "query": query })),
        StaffRequest::Award {
            key_id,
            medal,
            note,
        } => (
            "/v1/staff/award",
            json!({ "key_id": key_id, "medal": medal, "note": note }),
        ),
        StaffRequest::Unaward { key_id, medal } => (
            "/v1/staff/unaward",
            json!({ "key_id": key_id, "medal": medal }),
        ),
        StaffRequest::Unlock {
            key_id,
            unlock,
            note,
        } => (
            "/v1/staff/unlock",
            json!({ "key_id": key_id, "unlock": unlock, "note": note }),
        ),
        StaffRequest::Relock { key_id, unlock } => (
            "/v1/staff/relock",
            json!({ "key_id": key_id, "unlock": unlock }),
        ),
        StaffRequest::ClearAchievements { key_id, id } => (
            "/v1/staff/clear-achievements",
            json!({ "key_id": key_id, "id": id }),
        ),
        StaffRequest::ChatDelete { id } => ("/v1/staff/chat-delete", json!({ "id": id })),
        StaffRequest::ChatMute { key_id, muted } => (
            "/v1/staff/chat-mute",
            json!({ "key_id": key_id, "muted": muted }),
        ),
        StaffRequest::AvatarRemove { key_id } => {
            ("/v1/staff/avatar-remove", json!({ "key_id": key_id }))
        }
        StaffRequest::AvatarBlock { key_id, blocked } => (
            "/v1/staff/avatar-block",
            json!({ "key_id": key_id, "blocked": blocked }),
        ),
        StaffRequest::HolocronGive { key_id, tier, note } => (
            "/v1/staff/holocron-give",
            json!({ "key_id": key_id, "tier": tier, "note": note }),
        ),
        StaffRequest::HolocronRemove { key_id, id } => (
            "/v1/staff/holocron-remove",
            json!({ "key_id": key_id, "id": id }),
        ),
        StaffRequest::Verify { key_id, verified } => (
            "/v1/staff/verify",
            json!({ "key_id": key_id, "verified": verified }),
        ),
        StaffRequest::Merge { key_id, from } => {
            ("/v1/staff/merge", json!({ "key_id": key_id, "from": from }))
        }
        StaffRequest::Unlink { key_id } => ("/v1/staff/unlink", json!({ "key_id": key_id })),
    }
}

/// A staff query's path and body.
fn staff_query_call(query: &StaffQuery) -> (&'static str, Value) {
    match query {
        StaffQuery::RequestLogs { key_id, note } => (
            "/v1/staff/log-request",
            json!({ "key_id": key_id, "note": note }),
        ),
        StaffQuery::LogRequests => ("/v1/staff/log-requests", json!({})),
        StaffQuery::Log(id) => ("/v1/staff/log", json!({ "id": id })),
        StaffQuery::CrashGroups => ("/v1/staff/crash-groups", json!({})),
        StaffQuery::Crashes(signature) => ("/v1/staff/crashes", json!({ "signature": signature })),
        StaffQuery::Crash(id) => ("/v1/staff/crash", json!({ "id": id })),
    }
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, HubError> {
    serde_json::from_value(value).map_err(|error| HubError::Protocol(error.to_string()))
}

impl Hub for HttpHub {
    fn register(&mut self, identity: &Identity, name: Option<&str>) -> Result<Profile, HubError> {
        let body = match name {
            Some(name) => json!({ "name": name }),
            None => json!({}),
        };
        parse(self.send(Some(identity), "POST", "/v1/register", Some(body))?)
    }

    fn report(&mut self, identity: &Identity, report: &BugReport) -> Result<i64, HubError> {
        let mut body = json!({
            "text": report.text,
            "map": report.map,
            "build": report.build,
            "server": report.server,
        });
        if !report.name.is_empty() {
            body["name"] = json!(report.name);
        }
        let answer = self.send(Some(identity), "POST", "/v1/report", Some(body))?;
        answer
            .get("id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| HubError::Protocol("the report answer has no id".to_owned()))
    }

    fn settings_usage(&mut self, body: &Value) -> Result<u64, HubError> {
        let answer = self.send(None, "POST", "/v1/usage/settings", Some(body.clone()))?;
        Ok(answer["stored"].as_u64().unwrap_or(0))
    }

    fn donation_goal(&mut self) -> Result<crate::DonationGoal, HubError> {
        let answer = self.send(None, "GET", "/v1/donations/goal", None)?;
        crate::DonationGoal::parse(&answer)
            .ok_or_else(|| HubError::Protocol("the donation goal answer has no goal".to_owned()))
    }

    fn crash(&mut self, identity: &Identity, report: &CrashReport) -> Result<i64, HubError> {
        let answer = self.send(Some(identity), "POST", "/v1/crash", Some(report.body()))?;
        answer
            .get("id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| HubError::Protocol("the crash report answer has no id".to_owned()))
    }

    fn player_report(
        &mut self,
        identity: &Identity,
        report: &PlayerReport,
    ) -> Result<i64, HubError> {
        let answer = self.send(
            Some(identity),
            "POST",
            "/v1/player-report",
            Some(player_report_body(report)),
        )?;
        answer
            .get("id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| HubError::Protocol("the player report answer has no id".to_owned()))
    }

    fn note(&mut self, identity: &Identity, note: &WorldNote) -> Result<i64, HubError> {
        let mut body = json!({
            "text": note.text,
            "map": note.map,
            "build": note.build,
            "server": note.server,
            "view": note.view,
            "hit": note.hit,
            "normal": note.normal,
            "shader": note.shader,
            "surface": note.surface,
            "lighting": note.lighting,
            "distance": note.distance,
            "entity": note.entity,
        });
        if !note.name.is_empty() {
            body["name"] = json!(note.name);
        }
        note_context(&mut body, note);
        let answer = self.send(Some(identity), "POST", "/v1/note", Some(body))?;
        answer
            .get("id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| HubError::Protocol("the note answer has no id".to_owned()))
    }

    fn note_image(&mut self, identity: &Identity, id: i64, jpeg: &[u8]) -> Result<(), HubError> {
        let path = format!("/v1/note/{id}/image");
        self.send_bytes(Some(identity), "PUT", &path, jpeg, "image/jpeg")
            .map(|_| ())
    }

    fn set_bio(&mut self, identity: &Identity, bio: &str) -> Result<Profile, HubError> {
        let body = json!({ "bio": bio });
        parse(self.send(Some(identity), "PUT", "/v1/profile", Some(body))?)
    }

    fn set_achievements(
        &mut self,
        identity: &Identity,
        progress: &BTreeMap<String, u64>,
    ) -> Result<Vec<Achievement>, HubError> {
        let body = json!({ "progress": progress });
        let answer: Achievements =
            parse(self.send(Some(identity), "PUT", "/v1/achievements", Some(body))?)?;
        Ok(answer.achievements)
    }

    fn staff(
        &mut self,
        identity: &Identity,
        request: &StaffRequest,
    ) -> Result<Vec<Profile>, HubError> {
        let (path, body) = staff_call(request);
        let answer = self.send(Some(identity), "POST", path, Some(body))?;
        match request {
            StaffRequest::Search(_) => {
                let found: Players = parse(answer)?;
                Ok(found.players)
            }
            // The chat's moderation answers `{}`.
            StaffRequest::ChatDelete { .. } | StaffRequest::ChatMute { .. } => Ok(Vec::new()),
            _ => Ok(vec![parse(answer)?]),
        }
    }

    fn staff_query(
        &mut self,
        identity: &Identity,
        query: &StaffQuery,
    ) -> Result<StaffAnswer, HubError> {
        let (path, body) = staff_query_call(query);
        let body = body.to_string();
        let answer = self.send_with(
            Some(identity),
            "POST",
            path,
            body.as_bytes(),
            "application/json",
            STAFF_ANSWER_MAX,
            None,
        )?;
        Ok(match query {
            StaffQuery::RequestLogs { .. } | StaffQuery::Log(_) => {
                StaffAnswer::LogRequest(Box::new(parse::<LogRequestRow>(answer)?))
            }
            StaffQuery::LogRequests => {
                StaffAnswer::LogRequests(parse::<LogRequestRows>(answer)?.requests)
            }
            StaffQuery::CrashGroups => {
                StaffAnswer::CrashGroups(parse::<CrashGroups>(answer)?.groups)
            }
            StaffQuery::Crashes(_) => {
                StaffAnswer::Crashes(parse::<CrashSummaries>(answer)?.crashes)
            }
            StaffQuery::Crash(_) => StaffAnswer::Crash(Box::new(parse::<CrashDetail>(answer)?)),
        })
    }

    fn log_requests(&mut self, identity: &Identity) -> Result<Vec<LogRequest>, HubError> {
        let answer: LogRequests =
            parse(self.send(Some(identity), "GET", "/v1/log-requests", None)?)?;
        Ok(answer.requests)
    }

    fn send_logs(
        &mut self,
        identity: &Identity,
        id: i64,
        upload: &LogUpload,
    ) -> Result<(), HubError> {
        let body = upload.body(id).to_string();
        self.send_with(
            Some(identity),
            "POST",
            "/v1/logs",
            body.as_bytes(),
            "application/json",
            ANSWER_MAX,
            Some(LOGS_TIMEOUT),
        )
        .map(|_| ())
    }

    fn decline_logs(&mut self, identity: &Identity, id: i64) -> Result<(), HubError> {
        self.send(
            Some(identity),
            "POST",
            "/v1/logs/decline",
            Some(json!({ "request": id })),
        )
        .map(|_| ())
    }

    fn set_avatar(&mut self, identity: &Identity, png: &[u8]) -> Result<Profile, HubError> {
        parse(self.send_bytes(Some(identity), "PUT", "/v1/avatar", png, "image/png")?)
    }

    fn remove_avatar(&mut self, identity: &Identity) -> Result<Profile, HubError> {
        parse(self.send(Some(identity), "DELETE", "/v1/avatar", None)?)
    }

    fn avatar(&mut self, key_id: &str, version: &str) -> Result<Vec<u8>, HubError> {
        if !crate::avatar::valid_key_id(key_id) || !crate::avatar::valid_version(version) {
            return Err(HubError::Protocol(
                "a picture is named by a key id and a version".to_owned(),
            ));
        }
        let png = self.fetch(
            &crate::avatar::path(key_id, version),
            crate::avatar::ANSWER_MAX,
        )?;
        if !png.starts_with(crate::avatar::PNG_SIGNATURE) {
            return Err(HubError::Protocol("the picture is not a PNG".to_owned()));
        }
        Ok(png)
    }

    fn profile(&mut self, key_id: &str) -> Result<Profile, HubError> {
        if key_id.len() != 16 || !key_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(HubError::Protocol("a key id is 16 hex digits".to_owned()));
        }
        parse(self.send(None, "GET", &format!("/v1/profile/{key_id}"), None)?)
    }

    fn claim(
        &mut self,
        identity: &Identity,
        server: &str,
        slot: u8,
        name: &str,
        active: bool,
    ) -> Result<(), HubError> {
        let body = claim_body(server, slot, name, active);
        self.send(Some(identity), "POST", "/v1/claim", Some(body))
            .map(|_| ())
    }

    fn holocrons(&mut self, identity: &Identity) -> Result<HolocronState, HubError> {
        parse(self.send(Some(identity), "GET", "/v1/holocrons", None)?)
    }

    fn open_holocrons(&mut self, identity: &Identity, ids: &[u64]) -> Result<OpenAnswer, HubError> {
        let (path, body) = open_call(ids);
        parse(self.send(Some(identity), "POST", path, Some(body))?)
    }

    fn reveal_cards(&mut self, identity: &Identity) -> Result<crate::wire::RevealAnswer, HubError> {
        parse(self.send(Some(identity), "POST", "/v1/cards/reveal", Some(json!({})))?)
    }

    fn release(&mut self, identity: &Identity, server: &str) -> Result<(), HubError> {
        self.send(
            Some(identity),
            "DELETE",
            "/v1/claim",
            Some(json!({"server": server})),
        )
        .map(|_| ())
    }

    fn presence(&mut self, server: &str) -> Result<Vec<Presence>, HubError> {
        let query = encode_server(server);
        let answer = self.send(None, "GET", &format!("/v1/presence?server={query}"), None)?;
        parse(answer["players"].clone())
    }

    fn set_models(&mut self, identity: &Identity, models: &[String]) -> Result<u64, HubError> {
        let body = json!({ "models": models });
        let answer = self.send(Some(identity), "PUT", "/v1/models", Some(body))?;
        answer["stored"]
            .as_u64()
            .ok_or_else(|| HubError::Protocol("the models answer has no count".to_owned()))
    }

    fn model_check(
        &mut self,
        server: &str,
        slot: u8,
        model: &str,
    ) -> Result<(String, Option<bool>), HubError> {
        let path = format!(
            "/v1/models?server={}&slot={slot}&model={}",
            encode_server(server),
            encode_model(model)
        );
        let answer = self.send(None, "GET", &path, None)?;
        let claimed_name = answer["claimed_name"]
            .as_str()
            .ok_or_else(|| HubError::Protocol("the model answer has no name".to_owned()))?;
        Ok((claimed_name.to_owned(), answer["has"].as_bool()))
    }

    fn chat(&mut self, identity: &Identity, text: &str, name: &str) -> Result<u64, HubError> {
        let mut body = json!({ "text": text });
        if !name.is_empty() {
            body["name"] = json!(name);
        }
        let answer = self.send(Some(identity), "POST", "/v1/chat", Some(body))?;
        answer["id"]
            .as_u64()
            .ok_or_else(|| HubError::Protocol("the chat answer has no id".to_owned()))
    }

    fn emote(
        &mut self,
        identity: &Identity,
        server: &str,
        emote: &str,
        sync: Option<u8>,
    ) -> Result<u64, HubError> {
        let mut body = json!({ "server": server, "emote": emote });
        if let Some(sync) = sync {
            body["sync"] = json!(sync);
        }
        let answer = self.send(Some(identity), "POST", "/v1/emote", Some(body))?;
        answer["id"]
            .as_u64()
            .ok_or_else(|| HubError::Protocol("the emote answer has no id".to_owned()))
    }

    fn ping(
        &mut self,
        identity: &Identity,
        server: &str,
        pos: [i32; 3],
        scope: &str,
    ) -> Result<u64, HubError> {
        let body = json!({ "server": server, "pos": pos, "scope": scope });
        let answer = self.send(Some(identity), "POST", "/v1/ping", Some(body))?;
        answer["id"]
            .as_u64()
            .ok_or_else(|| HubError::Protocol("the ping answer has no id".to_owned()))
    }

    fn vitals(
        &mut self,
        identity: &Identity,
        server: &str,
        sharing: &Sharing,
    ) -> Result<Vec<SharedVitals>, HubError> {
        let mut body = json!({
            "server": server,
            "share": sharing.share,
            "estimates": sharing.estimates,
        });
        if let Some(vitals) = sharing.vitals.filter(|_| sharing.share) {
            body["vitals"] = serde_json::to_value(vitals)
                .map_err(|error| HubError::Protocol(error.to_string()))?;
        }
        let answer = self.send(Some(identity), "POST", "/v1/vitals", Some(body))?;
        parse(answer["players"].clone())
    }

    fn look(&mut self, identity: &Identity, server: &str, look: &Look) -> Result<u64, HubError> {
        let body = look_body(server, look);
        let answer = self.send(Some(identity), "POST", "/v1/look", Some(body))?;
        answer["id"]
            .as_u64()
            .ok_or_else(|| HubError::Protocol("the look answer has no id".to_owned()))
    }

    fn assets(&mut self, identity: &Identity) -> Result<Vec<Pack>, HubError> {
        crate::assets::manifest(self.send(Some(identity), "GET", "/v1/assets", None)?)
    }

    fn asset(&mut self, identity: &Identity, name: &str) -> Result<Vec<u8>, HubError> {
        if !crate::assets::valid_name(name) {
            return Err(HubError::Protocol(
                "a pack name is 1 to 32 of a to z, 0 to 9 and _".to_owned(),
            ));
        }
        let path = format!("/v1/assets/{name}");
        self.get_bytes(identity, &path, PACK_MAX, ASSET_TIMEOUT)
    }

    fn feed(
        &mut self,
        identity: &Identity,
        after: u64,
        server: Option<&str>,
        wait: u64,
    ) -> Result<Feed, HubError> {
        let path = feed_path(after, server, wait);
        parse(self.send(Some(identity), "GET", &path, None)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hub_address_must_be_https_or_local() {
        assert!(valid_base_url("https://hub.example"));
        assert!(valid_base_url("https://hub.example:8443/"));
        assert!(valid_base_url("http://localhost:8787"));
        assert!(valid_base_url("http://127.0.0.1:8787"));
        assert!(valid_base_url("http://[::1]:8787"));
        assert!(!valid_base_url("https://hub.example/prefix"));
        assert!(!valid_base_url("http://127.0.0.1.evil.example"));
        assert!(!valid_base_url("http://localhost.evil.example:80"));
        assert!(!valid_base_url("http://hub.example"));
        assert!(!valid_base_url("hub.example"));
        assert!(!valid_base_url("https://"));
        assert!(!valid_base_url("https://user@hub.example"));
        assert!(!valid_base_url("ftp://hub.example"));
        assert!(!valid_base_url(""));
    }

    #[test]
    fn a_plain_http_address_is_refused_at_construction() {
        assert!(HttpHub::new("http://hub.example", "t").is_err());
        assert!(HttpHub::new("https://hub.example/", "t").is_ok());
    }

    #[test]
    fn a_player_report_sends_the_protocols_fields_only() {
        let report = PlayerReport {
            category: crate::report::Category::Griefing,
            text: "Team killing all match".to_owned(),
            server: "1.2.3.4:29070".to_owned(),
            slot: 5,
            target_name: "^1Troll".to_owned(),
            level_time: Some(754),
            ..PlayerReport::default()
        };
        let body = player_report_body(&report);
        assert_eq!(
            body,
            json!({"category": "griefing", "text": "Team killing all match",
                   "server": "1.2.3.4:29070", "slot": 5, "target_name": "^1Troll",
                   "map": "", "build": "", "level_time": 754})
        );
        let named = PlayerReport {
            target_key_id: "0123456789abcdef".to_owned(),
            server_name: "^4JoF".to_owned(),
            name: "^2Sol".to_owned(),
            ..report
        };
        let body = player_report_body(&named);
        assert_eq!(body["target_key_id"], "0123456789abcdef");
        assert_eq!(body["server_name"], "^4JoF");
        assert_eq!(body["name"], "^2Sol");
    }

    #[test]
    fn a_note_sends_its_remaps_and_model_only_when_it_has_them() {
        let mut body = json!({"text": "too shiny"});
        note_context(&mut body, &WorldNote::default());
        assert_eq!(
            body,
            json!({"text": "too shiny"}),
            "an old hub still takes it"
        );
        let note = WorldNote {
            remapped: "textures/video/raven".into(),
            remaps: vec![crate::NoteRemap {
                source: "server".into(),
                from: "textures/imperial/basic_floor".into(),
                to: "textures/video/raven".into(),
            }],
            model: Some(crate::NoteModel {
                path: "models/map_objects/taspir/lamp.md3".into(),
                classname: "misc_model_static".into(),
                entity: Some(42),
                origin: Some([1.0, 2.0, 3.0]),
                skin: String::new(),
                shaders: vec![crate::NoteModelShader {
                    surface: "lamp".into(),
                    shader: "models/map_objects/taspir/lamp".into(),
                    remapped: String::new(),
                }],
            }),
            ..WorldNote::default()
        };
        note_context(&mut body, &note);
        assert_eq!(body["remapped"], "textures/video/raven");
        assert_eq!(
            body["remaps"],
            json!([{"source": "server", "from": "textures/imperial/basic_floor",
                    "to": "textures/video/raven"}])
        );
        assert_eq!(
            body["model"],
            json!({"path": "models/map_objects/taspir/lamp.md3",
                   "classname": "misc_model_static", "entity": 42, "origin": [1.0, 2.0, 3.0],
                   "shaders": [{"surface": "lamp", "shader": "models/map_objects/taspir/lamp",
                                "remapped": ""}]})
        );
    }

    #[test]
    fn the_feed_path_carries_after_wait_and_the_server() {
        assert_eq!(feed_path(0, None, 25), "/v1/feed?after=0&wait=25");
        assert_eq!(
            feed_path(14, Some("1.2.3.4:29070"), 25),
            "/v1/feed?after=14&wait=25&server=1.2.3.4%3A29070"
        );
        assert_eq!(
            feed_path(1, Some("[::1]:29070"), 0),
            "/v1/feed?after=1&wait=0&server=%5B%3A%3A1%5D%3A29070"
        );
    }

    #[test]
    fn staff_chat_requests_send_the_protocols_fields() {
        assert_eq!(
            staff_call(&StaffRequest::ChatDelete { id: 12 }),
            ("/v1/staff/chat-delete", json!({"id": 12}))
        );
        assert_eq!(
            staff_call(&StaffRequest::ChatMute {
                key_id: "0123456789abcdef".into(),
                muted: true
            }),
            (
                "/v1/staff/chat-mute",
                json!({"key_id": "0123456789abcdef", "muted": true})
            )
        );
        assert_eq!(
            staff_call(&StaffRequest::Search("so".into())),
            ("/v1/staff/search", json!({"query": "so"}))
        );
        assert_eq!(
            staff_call(&StaffRequest::AvatarRemove {
                key_id: "0123456789abcdef".into()
            }),
            (
                "/v1/staff/avatar-remove",
                json!({"key_id": "0123456789abcdef"})
            )
        );
        assert_eq!(
            staff_call(&StaffRequest::AvatarBlock {
                key_id: "0123456789abcdef".into(),
                blocked: true
            }),
            (
                "/v1/staff/avatar-block",
                json!({"key_id": "0123456789abcdef", "blocked": true})
            )
        );
    }

    #[test]
    fn a_look_sends_exactly_the_protocols_fields() {
        let look = Look {
            saber: "saber_sun".to_owned(),
            illuminate: true,
            saber_off: Vec::new(),
        };
        assert_eq!(
            look_body("1.2.3.4:29070", &look),
            json!({"server": "1.2.3.4:29070", "saber": "saber_sun", "illuminate": true})
        );
        // The options switched off go only when there are some, with a blade skin.
        let off = Look {
            saber_off: vec!["glint".to_owned(), "haze".to_owned()],
            ..look.clone()
        };
        assert_eq!(
            look_body("1.2.3.4:29070", &off),
            json!({"server": "1.2.3.4:29070", "saber": "saber_sun", "illuminate": true,
                "saber_off": ["glint", "haze"]})
        );
        let stock = Look {
            saber: String::new(),
            ..off
        };
        assert!(
            look_body("1.2.3.4:29070", &stock)
                .get("saber_off")
                .is_none()
        );
        assert_eq!(
            look_body("[::1]:29070", &Look::default()).to_string(),
            r#"{"illuminate":false,"saber":"","server":"[::1]:29070"}"#
        );
    }

    #[test]
    fn staff_unlocks_send_the_protocols_fields() {
        assert_eq!(
            staff_call(&StaffRequest::Unlock {
                key_id: "0123456789abcdef".into(),
                unlock: "saber_sun".into(),
                note: "Thanks".into(),
            }),
            (
                "/v1/staff/unlock",
                json!({"key_id": "0123456789abcdef", "unlock": "saber_sun", "note": "Thanks"})
            )
        );
        assert_eq!(
            staff_call(&StaffRequest::Relock {
                key_id: "0123456789abcdef".into(),
                unlock: "saber_sun".into(),
            }),
            (
                "/v1/staff/relock",
                json!({"key_id": "0123456789abcdef", "unlock": "saber_sun"})
            )
        );
    }

    #[test]
    fn a_claim_says_active_only_when_it_is() {
        assert_eq!(
            claim_body("1.2.3.4:29070", 3, "Sol", false),
            json!({"server": "1.2.3.4:29070", "slot": 3, "name": "Sol"}),
            "the body of a hub from before holocrons"
        );
        assert_eq!(
            claim_body("1.2.3.4:29070", 3, "Sol", true),
            json!({"server": "1.2.3.4:29070", "slot": 3, "name": "Sol", "active": true})
        );
    }

    #[test]
    fn opening_sends_the_ids_and_reads_what_they_brought() {
        assert_eq!(
            open_call(&[7, 9]),
            ("/v1/holocrons/open", json!({ "ids": [7, 9] }))
        );
        let answer: OpenAnswer = serde_json::from_str(
            r#"{"opened":[{"holocron":7,"tier":"rare","card":{"id":31,"rarity":"uncommon","finish":"holo","edition":"alpha"},"credits":42}],
                "profile":{"key_id":"0123456789abcdef","key":"k","name":"Sol","bio":"","verified":false,"created":1,
                  "credits":42,"cards":[{"card":null,"rarity":"uncommon","finish":"holo","edition":"alpha","count":1,"first":5,"last":5}],
                  "holocron_counts":{"uncommon":0,"rare":0,"legendary":0,"mythical":0},
                  "holocrons":[{"id":7,"tier":"rare","dropped":1,"source":"play","note":"","opened":5}]}}"#,
        )
        .unwrap();
        assert_eq!(answer.opened.len(), 1);
        let opened = &answer.opened[0];
        assert_eq!(
            (opened.holocron, opened.tier.as_str(), opened.credits),
            (7, "rare", 42)
        );
        assert_eq!(
            (
                opened.card.as_ref().unwrap().id,
                opened.card.as_ref().unwrap().rarity.as_str(),
                opened.card.as_ref().unwrap().finish.as_str(),
                opened.card.as_ref().unwrap().edition.as_str()
            ),
            (31, "uncommon", "holo", "alpha")
        );
        assert_eq!(answer.profile.credits, 42);
        assert_eq!(answer.profile.cards[0].card, None, "sealed");
        assert_eq!(answer.profile.cards[0].count, 1);
        assert_eq!(answer.profile.holocrons[0].opened, Some(5));
        assert!(!answer.profile.holocrons[0].is_sealed());
    }

    #[test]
    fn staff_holocron_requests_send_the_protocols_fields() {
        assert_eq!(
            staff_call(&StaffRequest::HolocronGive {
                key_id: "0123456789abcdef".into(),
                tier: "legendary".into(),
                note: "For the fog bug".into(),
            }),
            (
                "/v1/staff/holocron-give",
                json!({"key_id": "0123456789abcdef", "tier": "legendary",
                       "note": "For the fog bug"})
            )
        );
        assert_eq!(
            staff_call(&StaffRequest::HolocronRemove {
                key_id: "0123456789abcdef".into(),
                id: 41,
            }),
            (
                "/v1/staff/holocron-remove",
                json!({"key_id": "0123456789abcdef", "id": 41})
            )
        );
    }

    #[test]
    fn staff_verify_merge_and_unlink_send_the_protocols_fields() {
        assert_eq!(
            staff_call(&StaffRequest::Verify {
                key_id: "0123456789abcdef".into(),
                verified: true,
            }),
            (
                "/v1/staff/verify",
                json!({"key_id": "0123456789abcdef", "verified": true})
            )
        );
        assert_eq!(
            staff_call(&StaffRequest::Merge {
                key_id: "0123456789abcdef".into(),
                from: "fedcba9876543210".into(),
            }),
            (
                "/v1/staff/merge",
                json!({"key_id": "0123456789abcdef", "from": "fedcba9876543210"})
            )
        );
        assert_eq!(
            staff_call(&StaffRequest::Unlink {
                key_id: "fedcba9876543210".into(),
            }),
            ("/v1/staff/unlink", json!({"key_id": "fedcba9876543210"}))
        );
    }

    #[test]
    fn staff_queries_send_the_protocols_fields() {
        assert_eq!(
            staff_query_call(&StaffQuery::RequestLogs {
                key_id: "0123456789abcdef".into(),
                note: "your freeze".into(),
            }),
            (
                "/v1/staff/log-request",
                json!({"key_id": "0123456789abcdef", "note": "your freeze"})
            )
        );
        assert_eq!(
            staff_query_call(&StaffQuery::LogRequests),
            ("/v1/staff/log-requests", json!({}))
        );
        assert_eq!(
            staff_query_call(&StaffQuery::Log(4)),
            ("/v1/staff/log", json!({"id": 4}))
        );
        assert_eq!(
            staff_query_call(&StaffQuery::CrashGroups),
            ("/v1/staff/crash-groups", json!({}))
        );
        assert_eq!(
            staff_query_call(&StaffQuery::Crashes(String::new())),
            ("/v1/staff/crashes", json!({"signature": ""}))
        );
        assert_eq!(
            staff_query_call(&StaffQuery::Crash(7)),
            ("/v1/staff/crash", json!({"id": 7}))
        );
    }

    #[test]
    fn a_log_upload_sends_the_protocols_fields() {
        let upload = LogUpload {
            logs: "a\n".into(),
            build: "2026.1011.2".into(),
            os: "Windows 11".into(),
            gpu: "RTX".into(),
        };
        assert_eq!(
            upload.body(5),
            json!({"request": 5, "logs": "a\n", "build": "2026.1011.2",
                   "os": "Windows 11", "gpu": "RTX"})
        );
    }

    #[test]
    fn a_key_id_is_checked_before_a_request_is_made() {
        let mut hub = HttpHub::new("https://hub.example", "t").unwrap();
        assert!(matches!(hub.profile("../etc"), Err(HubError::Protocol(_))));
        assert!(matches!(
            hub.avatar("../etc", "0123456789abcdef"),
            Err(HubError::Protocol(_))
        ));
        assert!(matches!(
            hub.avatar("0123456789abcdef", "../x"),
            Err(HubError::Protocol(_))
        ));
        let identity = Identity::from_seed([7; 32]);
        assert!(matches!(
            hub.asset(&identity, "../etc"),
            Err(HubError::Protocol(_))
        ));
    }
}
