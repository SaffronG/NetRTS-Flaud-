//! HTTP client for the NetRts server.
//!
//! Covers exactly the endpoints the Python reference bot uses:
//!
//! | Method | Path                                         | Function                       |
//! |--------|----------------------------------------------|--------------------------------|
//! | POST   | `/api/v1/players`                            | [`Api::register_bot`]          |
//! | POST   | `/api/v1/matches`                            | [`Api::create_match`]          |
//! | POST   | `/api/v1/matches/{id}/join`                  | [`Api::join_match`]            |
//! | GET    | `/api/v1/rules`                              | [`Api::get_rules`]             |
//! | GET    | `/api/v1/matches/{id}/map`                   | [`Api::get_map`]               |
//! | GET    | `/api/v1/matches/{id}/state?waitForTick=N`   | [`Api::get_state`]             |
//! | POST   | `/api/v1/matches/{id}/commands`              | [`Api::submit_commands`]       |
//! | GET    | `/api/v1/matches/{id}/result`                | [`Api::get_result`]            |
//!
//! Every call returns [`ApiResult`]. A non-2xx response becomes
//! [`ApiError::Status`] carrying the server's `{"code", "message"}` body, so
//! callers can branch on codes like `NAME_TAKEN` or `MATCH_NOT_STARTED`.

use crate::logic::models::*;
use reqwest::{Client, RequestBuilder, StatusCode, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt;
use std::time::Duration;

/// Per-request timeout. The state endpoint long-polls for up to 30 s, so this
/// matches the Python bot's 60 s to leave room for a full poll plus latency.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Error returned by every [`Api`] call.
#[derive(Debug)]
pub enum ApiError {
    /// No usable response: DNS failure, refused connection, timeout, or a
    /// success body that didn't match the expected model.
    Http(reqwest::Error),
    /// The server answered with a non-2xx status.
    ///
    /// `code` and `message` come from the JSON error body. If the body is
    /// missing or isn't JSON, `code` is `"HTTP_ERROR"` and `message` is the
    /// status text, the same fallback the Python bot uses.
    Status {
        status: StatusCode,
        code: String,
        message: String,
    },
}

impl ApiError {
    /// HTTP status of the failed response, if there was one.
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            ApiError::Status { status, .. } => Some(*status),
            ApiError::Http(e) => e.status(),
        }
    }

    /// Server error code such as `NAME_TAKEN` or `MATCH_NOT_STARTED`.
    ///
    /// `None` for transport errors, which never reached the server's error handler.
    pub fn code(&self) -> Option<&str> {
        match self {
            ApiError::Status { code, .. } => Some(code),
            ApiError::Http(_) => None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Status { status, code, message } => {
                write!(f, "API error {} {code}: {message}", status.as_u16())
            }
            ApiError::Http(e) if e.is_connect() || e.is_timeout() => {
                write!(f, "Cannot reach server: {e}")
            }
            ApiError::Http(e) => write!(f, "Request failed: {e}"),
        }
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApiError::Http(e) => Some(e),
            ApiError::Status { .. } => None,
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Http(e)
    }
}

/// Result type for every [`Api`] call.
pub type ApiResult<T> = Result<T, ApiError>;

/// Client for one NetRts server and one player.
///
/// Cheap to clone: the inner [`Client`] is reference-counted, so clones share
/// one connection pool.
#[derive(Clone)]
pub struct Api {
    /// Name of the selected bot. Display only; never sent to the server.
    pub bot_name: String,
    /// Base URL of the server, always ending in `/`.
    pub server: Url,
    /// Player API key, sent as `Authorization: Bearer {key}`.
    ///
    /// Leave empty before registering: the header is skipped when the key is empty.
    pub api_key: String,
    /// Match id for the currently opened match
    pub match_id: String,
    client: Client,
}

impl Api {
    /// Creates a client for the server at `server_addr`, authenticating with `key`.
    ///
    /// The address is normalized before parsing:
    /// - A missing scheme becomes `http://` for `localhost` / `127.0.0.1`, and
    ///   `https://` for anything else.
    /// - A trailing `/` is added, so a base path like `/rts` is kept when
    ///   endpoint paths are joined onto it.
    ///
    /// # Panics
    ///
    /// Panics if the normalized address still isn't a valid URL.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let api = Api::new("Flaud".to_string(), "netrts.example.io", "my-api-key");
    /// assert_eq!(api.server.as_str(), "https://netrts.example.io/");
    /// ```
    pub fn new<S, K>(name: String, server_addr: S, key: K) -> Self
    where
        S: AsRef<str>,
        K: Into<String>,
    {
        let addr = normalize_server(server_addr.as_ref());
        Self {
            bot_name: name,
            server: Url::parse(&addr)
                .unwrap_or_else(|e| panic!("Invalid server URL {addr:?}: {e}")),
            api_key: key.into(),
            match_id: String::new(),
            client: Client::new(),
        }
    }

    /// Registers a new player named `name` (`POST /api/v1/players`).
    ///
    /// Call this on a client with an empty [`api_key`](Self::api_key). The
    /// returned key is shown only once, so store it.
    ///
    /// # Errors
    ///
    /// - [`ApiError::Status`] with code `NAME_TAKEN` if the name is already registered.
    /// - Any other [`ApiError`] for transport or server failures.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut api = Api::new("Flaud".to_string(), "http://localhost:5080", "");
    /// let reg = api.register_bot("Flaud").await?;
    /// api.api_key = reg.api_key;
    /// ```
    pub async fn register_bot(&self, name: impl Into<String>) -> ApiResult<RegisterPlayerResponse> {
        let payload = RegisterPlayerRequest { name: name.into() };
        self.send(self.client.post(self.url("api/v1/players")).json(&payload))
            .await
    }

    /// Creates a new match (`POST /api/v1/matches`).
    ///
    /// The Python bot sends one of two bodies:
    /// - vs a house bot: `{"houseBots": ["sitter"], "settings": {...}}`
    /// - open match for others to join: `{"maxPlayers": 2, "settings": {...}}`
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::Status`] if the server rejects the settings or the
    /// API key, or [`ApiError::Http`] on transport failure.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let request = CreateMatchRequest {
    ///     house_bots: Some(vec!["sitter".into()]),
    ///     settings: Some(MatchSettingsDto { seed: Some(1234), ..Default::default() }),
    ///     ..Default::default()
    /// };
    /// let summary = api.create_match(&request).await?;
    /// println!("Created match {}", summary.match_id);
    /// ```
    pub async fn create_match(&self, body: &CreateMatchRequest) -> ApiResult<MatchSummaryDto> {
        self.send(self.client.post(self.url("api/v1/matches")).json(body))
            .await
    }

    /// Lists the matches the server has in memory (`GET /api/v1/matches`):
    /// waiting, running, or recently finished.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// for m in api.list_matches().await? {
    ///     println!("{} {:?}", m.match_id, m.name);
    /// }
    /// ```
    pub async fn list_matches(&self) -> ApiResult<Vec<MatchSummaryDto>> {
        self.send(self.client.get(self.url("api/v1/matches"))).await
    }

    /// Finds the id of the match called `name` (case-insensitive), preferring
    /// one that is still waiting for players. Returns `None` if no match has
    /// that name.
    pub async fn find_match_by_name(&self, name: &str) -> ApiResult<Option<String>> {
        let matches = self.list_matches().await?;
        let named: Vec<&MatchSummaryDto> = matches
            .iter()
            .filter(|m| m.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)))
            .collect();
        Ok(named
            .iter()
            .find(|m| m.status == MatchStatus::Waiting)
            .or(named.first())
            .map(|m| m.match_id.clone()))
    }

    /// Joins the open match `match_id` (`POST /api/v1/matches/{id}/join`).
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::Status`] if the match doesn't exist, is full, or has
    /// already started.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let summary = api.join_match("3f2a9c1e-...").await?;
    /// println!("Joined match {}", summary.match_id);
    /// ```
    pub async fn join_match(&self, match_id: &str) -> ApiResult<MatchSummaryDto> {
        self.send(self.client.post(self.match_url(match_id, "join")))
            .await
    }

    /// Fetches the static game rules (`GET /api/v1/rules`): unit, building and
    /// upgrade stats plus economy constants.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let rules = api.get_rules().await?;
    /// let soldier_cost = rules.unit(UnitType::Soldier).map(|u| u.cost);
    /// ```
    pub async fn get_rules(&self) -> ApiResult<RulesDto> {
        self.send(self.client.get(self.url("api/v1/rules"))).await
    }

    /// Fetches the terrain and start positions (`GET /api/v1/matches/{id}/map`).
    ///
    /// The map only exists once the match has started, so call this after the
    /// first successful [`get_state`](Self::get_state).
    pub async fn get_map(&self, match_id: &str) -> ApiResult<MapDto> {
        self.send(self.client.get(self.match_url(match_id, "map")))
            .await
    }

    /// Long-polls the fog-of-war game state
    /// (`GET /api/v1/matches/{id}/state?waitForTick=N[&sinceTick=M]`).
    ///
    /// The server holds the request (up to 30 s) until tick `wait_for_tick`
    /// exists. `since_tick` limits `events` to ones after that tick, so each
    /// event is seen once.
    ///
    /// # Errors
    ///
    /// Before the match starts, `wait_for_tick = 0` fails with code
    /// `MATCH_NOT_STARTED`. Retry on that code until the match fills, as the
    /// Python `wait_for_start` does.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut state = loop {
    ///     match api.get_state(&id, 0, None).await {
    ///         Err(e) if e.code() == Some("MATCH_NOT_STARTED") => continue,
    ///         other => break other?,
    ///     }
    /// };
    /// while state.status != MatchStatus::Completed {
    ///     let tick = state.tick;
    ///     state = api.get_state(&id, tick + 1, Some(tick)).await?;
    /// }
    /// ```
    pub async fn get_state(
        &self,
        match_id: &str,
        wait_for_tick: i32,
        since_tick: Option<i32>,
    ) -> ApiResult<GameStateDto> {
        let mut url = self.match_url(match_id, "state");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("waitForTick", &wait_for_tick.to_string());
            if let Some(since) = since_tick {
                query.append_pair("sinceTick", &since.to_string());
            }
        }
        self.send(self.client.get(url)).await
    }

    /// Queues commands for this tick (`POST /api/v1/matches/{id}/commands`).
    ///
    /// The response has one [`CommandResult`] per command, in order. A rejected
    /// command doesn't fail the call; check `accepted` and `error` on each result.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let commands = [
    ///     CommandRequest::gather(vec![worker.id], deposit.id),
    ///     CommandRequest::produce(cc.id, UnitType::Worker, 1),
    /// ];
    /// let reply = api.submit_commands(&id, &commands).await?;
    /// for r in reply.results.iter().filter(|r| !r.accepted) {
    ///     eprintln!("command {} rejected: {:?}", r.index, r.error);
    /// }
    /// ```
    pub async fn submit_commands(
        &self,
        match_id: &str,
        commands: &[CommandRequest],
    ) -> ApiResult<SubmitCommandsResponse> {
        #[derive(Serialize)]
        struct Body<'a> {
            commands: &'a [CommandRequest],
        }
        let req = self
            .client
            .post(self.match_url(match_id, "commands"))
            .json(&Body { commands });
        self.send(req).await
    }

    /// Fetches the final outcome and per-player scores
    /// (`GET /api/v1/matches/{id}/result`).
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::Status`] if the match hasn't finished yet.
    pub async fn get_result(&self, match_id: &str) -> ApiResult<MatchResultDto> {
        self.send(self.client.get(self.match_url(match_id, "result")))
            .await
    }

    /// Resolves a relative endpoint path against the base URL.
    fn url(&self, path: &str) -> Url {
        self.server.join(path).expect("valid endpoint path")
    }

    /// `api/v1/matches/{match_id}/{action}`, with the id percent-encoded as one
    /// path segment so a pasted id can't change the route.
    fn match_url(&self, match_id: &str, action: &str) -> Url {
        let mut url = self.url("api/v1/matches/");
        url.path_segments_mut()
            .expect("http(s) URLs have path segments")
            .pop_if_empty()
            .push(match_id)
            .push(action);
        url
    }

    /// Adds auth and the timeout, sends `req`, and decodes either the success
    /// body as `T` or the error body as [`ApiError::Status`].
    async fn send<T: DeserializeOwned>(&self, req: RequestBuilder) -> ApiResult<T> {
        let mut req = req.timeout(REQUEST_TIMEOUT);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp.json::<T>().await?);
        }

        let body = resp.bytes().await.unwrap_or_default();
        Err(error_from_body(status, &body))
    }
}

/// Adds a scheme if missing and guarantees a trailing `/`.
fn normalize_server(addr: &str) -> String {
    let addr = addr.trim();
    let mut url = if addr.contains("://") {
        addr.to_string()
    } else if addr.starts_with("localhost") || addr.starts_with("127.0.0.1") {
        format!("http://{addr}")
    } else {
        format!("https://{addr}")
    };
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// Builds [`ApiError::Status`] from an error response body.
fn error_from_body(status: StatusCode, body: &[u8]) -> ApiError {
    let parsed: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let field = |k: &str| parsed.get(k).and_then(|v| v.as_str()).map(str::to_owned);
    ApiError::Status {
        status,
        code: field("code").unwrap_or_else(|| "HTTP_ERROR".into()),
        message: field("message")
            .unwrap_or_else(|| status.canonical_reason().unwrap_or("").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn api(server: &MockServer) -> Api {
        Api::new("Flaud".to_string(), server.uri(), "test-key")
    }

    fn ok(body: Value) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(body)
    }

    fn error(status: u16, code: &str, message: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_json(json!({ "code": code, "message": message }))
    }

    fn summary_json(id: &str) -> Value {
        json!({
            "matchId": id,
            "createdAt": "2026-10-11T00:00:00Z",
            "status": "Waiting",
            "mapWidth": 32, "mapHeight": 32,
            "maxPlayers": 2, "maxTicks": 2000,
            "seed": 1234, "tick": 0, "tickIntervalMs": 100,
            "outcome": null,
            "players": []
        })
    }

    fn unit_json(id: i32, owner: i32, kind: &str) -> Value {
        json!({
            "id": id, "type": kind, "owner": owner,
            "x": 3, "y": 3, "hp": 40, "maxHp": 40,
            "activity": "Idle", "carrying": 0,
            "destination": null, "targetId": null
        })
    }

    fn state_json(tick: i32, status: &str) -> Value {
        json!({
            "you": {
                "playerId": "p1", "name": "Flaud", "slot": 0,
                "resources": 50, "storageCapacity": 1000, "incomePerMinute": 0,
                "queueCapacity": 20, "queuedCommands": 0,
                "researching": [], "upgrades": []
            },
            "matchId": "m1", "status": status, "tick": tick,
            "maxTicks": 2000, "tickIntervalMs": 100,
            "mapWidth": 32, "mapHeight": 32,
            "outcome": null,
            "units": [unit_json(1, 0, "Worker"), unit_json(2, 0, "Worker"), unit_json(9, 1, "Soldier")],
            "buildings": [], "rememberedBuildings": [],
            "resources": [{ "id": 7, "x": 5, "y": 5, "remaining": 500 }],
            "events": [], "players": [], "visibility": []
        })
    }

    fn player_result_json(id: &str, name: &str, slot: i32, winner: bool) -> Value {
        json!({
            "playerId": id, "name": name, "slot": slot, "winner": winner,
            "score": { "destruction": 300, "economy": 1200, "survival": 400, "total": 1900 },
            "unitsProduced": 14, "unitsLost": 3, "unitsKilled": 9,
            "buildingsLost": 0, "buildingsDestroyed": 2
        })
    }

    #[test]
    fn server_address_gets_scheme_and_trailing_slash() {
        let cases = [
            ("netrts.example.io", "https://netrts.example.io/"),
            ("localhost:5080", "http://localhost:5080/"),
            ("127.0.0.1:5080", "http://127.0.0.1:5080/"),
            ("http://example.io:9000", "http://example.io:9000/"),
            ("  https://example.io/  ", "https://example.io/"),
        ];
        for (input, expected) in cases {
            assert_eq!(Api::new(String::new(), input, "").server.as_str(), expected, "{input}");
        }
    }

    #[test]
    fn base_path_is_kept_when_joining_endpoints() {
        let api = Api::new(String::new(), "http://example.io/rts", "");
        assert_eq!(api.url("api/v1/rules").as_str(), "http://example.io/rts/api/v1/rules");
        assert_eq!(
            api.match_url("m1", "state").as_str(),
            "http://example.io/rts/api/v1/matches/m1/state"
        );
    }

    #[test]
    fn match_id_is_encoded_as_one_segment() {
        let api = Api::new(String::new(), "http://example.io", "");
        assert_eq!(
            api.match_url("../players", "join").as_str(),
            "http://example.io/api/v1/matches/..%2Fplayers/join"
        );
    }

    #[test]
    #[should_panic(expected = "Invalid server URL")]
    fn unparseable_server_panics_with_address() {
        Api::new(String::new(), "http://", "");
    }

    #[tokio::test]
    async fn authenticated_calls_send_bearer_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/rules"))
            .and(header("Authorization", "Bearer test-key"))
            .respond_with(ok(json!({
                "economy": {
                    "gatherPerTick": 1, "maxProductionQueue": 5,
                    "startingResources": 50, "startingWorkers": 4, "workerCarryCapacity": 5
                },
                "buildings": [], "notes": ["test rules"], "units": [], "upgrades": []
            })))
            .expect(1)
            .mount(&server)
            .await;

        let rules = api(&server).get_rules().await.unwrap();
        assert_eq!(rules.economy.starting_workers, 4);
        assert_eq!(rules.notes, ["test rules"]);
    }

    #[tokio::test]
    async fn register_sends_name_without_auth_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/players"))
            .and(body_json(json!({ "name": "Flaud" })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "playerId": "p1", "name": "Flaud", "apiKey": "fresh-key"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let unregistered = Api::new("Flaud".to_string(), server.uri(), "");
        let reg = unregistered.register_bot("Flaud").await.unwrap();
        assert_eq!(reg.api_key, "fresh-key");

        let requests = server.received_requests().await.unwrap();
        assert!(requests[0].headers.get("authorization").is_none());
    }

    #[tokio::test]
    async fn register_name_taken_is_reported_by_code() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/players"))
            .respond_with(error(409, "NAME_TAKEN", "That name is taken."))
            .mount(&server)
            .await;

        let err = api(&server).register_bot("Flaud").await.unwrap_err();
        assert_eq!(err.status(), Some(StatusCode::CONFLICT));
        assert_eq!(err.code(), Some("NAME_TAKEN"));
        assert_eq!(err.to_string(), "API error 409 NAME_TAKEN: That name is taken.");
    }

    #[tokio::test]
    async fn non_json_error_body_falls_back_like_python() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/rules"))
            .respond_with(ResponseTemplate::new(502).set_body_string("upstream died"))
            .mount(&server)
            .await;

        let err = api(&server).get_rules().await.unwrap_err();
        assert_eq!(err.code(), Some("HTTP_ERROR"));
        assert_eq!(err.to_string(), "API error 502 HTTP_ERROR: Bad Gateway");
    }

    #[tokio::test]
    async fn wrong_success_body_is_an_http_decode_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/rules"))
            .respond_with(ok(json!({ "unexpected": true })))
            .mount(&server)
            .await;

        let err = api(&server).get_rules().await.unwrap_err();
        assert!(matches!(&err, ApiError::Http(e) if e.is_decode()), "{err:?}");
        assert_eq!(err.code(), None);
    }

    #[tokio::test]
    async fn unreachable_server_is_an_http_error() {
        let api = Api::new(String::new(), "http://127.0.0.1:1", "k");
        let err = api.get_rules().await.unwrap_err();
        assert!(matches!(err, ApiError::Http(_)));
        assert_eq!(err.code(), None);
        assert!(err.to_string().starts_with("Cannot reach server"), "{err}");
    }

    #[tokio::test]
    async fn create_match_vs_house_bot_sends_python_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/matches"))
            .and(body_json(json!({ "houseBots": ["sitter"], "settings": { "seed": 1234 } })))
            .respond_with(ok(summary_json("m1")))
            .expect(1)
            .mount(&server)
            .await;

        let req = CreateMatchRequest {
            house_bots: Some(vec!["sitter".into()]),
            settings: Some(MatchSettingsDto { seed: Some(1234), ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(api(&server).create_match(&req).await.unwrap().match_id, "m1");
    }

    #[tokio::test]
    async fn create_open_match_sends_python_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/matches"))
            .and(body_json(json!({ "maxPlayers": 3, "settings": { "tickIntervalMs": 50 } })))
            .respond_with(ok(summary_json("m2")))
            .expect(1)
            .mount(&server)
            .await;

        let req = CreateMatchRequest {
            max_players: Some(3),
            settings: Some(MatchSettingsDto { tick_interval_ms: Some(50), ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(api(&server).create_match(&req).await.unwrap().match_id, "m2");
    }

    #[tokio::test]
    async fn join_match_posts_to_join_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/matches/m1/join"))
            .respond_with(ok(summary_json("m1")))
            .expect(1)
            .mount(&server)
            .await;

        assert_eq!(api(&server).join_match("m1").await.unwrap().match_id, "m1");
    }

    #[tokio::test]
    async fn list_matches_parses_names() {
        let server = MockServer::start().await;
        let mut named = summary_json("m1");
        named["name"] = json!("PostFallBreak");
        Mock::given(method("GET"))
            .and(path("/api/v1/matches"))
            .respond_with(ok(json!([named, summary_json("m2")])))
            .mount(&server)
            .await;

        let matches = api(&server).list_matches().await.unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].name.as_deref(), Some("PostFallBreak"));
        assert_eq!(matches[1].name, None);
    }

    #[tokio::test]
    async fn find_match_by_name_prefers_waiting_and_ignores_case() {
        let server = MockServer::start().await;
        let mut done = summary_json("old");
        done["name"] = json!("Scrim");
        done["status"] = json!("Completed");
        let mut open = summary_json("new");
        open["name"] = json!("Scrim");
        Mock::given(method("GET"))
            .and(path("/api/v1/matches"))
            .respond_with(ok(json!([done, open])))
            .mount(&server)
            .await;

        let api = api(&server);
        assert_eq!(api.find_match_by_name("scrim").await.unwrap().as_deref(), Some("new"));
        assert_eq!(api.find_match_by_name("missing").await.unwrap(), None);
    }

    #[tokio::test]
    async fn join_match_returns_error_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/matches/nope/join"))
            .respond_with(error(404, "MATCH_NOT_FOUND", "No such match"))
            .mount(&server)
            .await;

        let err = api(&server).join_match("nope").await.unwrap_err();
        assert_eq!(err.status(), Some(StatusCode::NOT_FOUND));
        assert_eq!(err.code(), Some("MATCH_NOT_FOUND"));
    }

    #[tokio::test]
    async fn get_map_parses_terrain_and_starts() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/matches/m1/map"))
            .respond_with(ok(json!({
                "width": 3, "height": 2,
                "startPositions": [{ "x": 0, "y": 0 }, { "x": 2, "y": 1 }],
                "terrain": ["..#", "..."]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let map = api(&server).get_map("m1").await.unwrap();
        assert_eq!((map.width, map.height), (3, 2));
        assert_eq!(map.start_positions.len(), 2);
        assert!(!map.is_passable(2, 0));
        assert!(map.is_passable(2, 1));
    }

    #[tokio::test]
    async fn get_state_sends_long_poll_parameters() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/matches/m1/state"))
            .and(query_param("waitForTick", "5"))
            .and(query_param("sinceTick", "4"))
            .respond_with(ok(state_json(5, "Active")))
            .expect(1)
            .mount(&server)
            .await;

        let state = api(&server).get_state("m1", 5, Some(4)).await.unwrap();
        assert_eq!(state.tick, 5);
        assert_eq!(state.status, MatchStatus::Active);
        assert_eq!(state.my_units().count(), 2);
        assert_eq!(state.enemy_units().count(), 1);
    }

    #[tokio::test]
    async fn get_state_omits_since_tick_when_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/matches/m1/state"))
            .respond_with(ok(state_json(0, "Active")))
            .mount(&server)
            .await;

        api(&server).get_state("m1", 0, None).await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests[0].url.query(), Some("waitForTick=0"));
    }

    #[tokio::test]
    async fn get_state_before_start_reports_match_not_started() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/matches/m1/state"))
            .respond_with(error(409, "MATCH_NOT_STARTED", "Waiting for players"))
            .mount(&server)
            .await;

        let err = api(&server).get_state("m1", 0, None).await.unwrap_err();
        assert_eq!(err.code(), Some("MATCH_NOT_STARTED"));
    }

    #[tokio::test]
    async fn submit_commands_sends_python_shaped_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/matches/m1/commands"))
            .and(body_json(json!({
                "commands": [
                    { "type": "Gather", "unitIds": [1], "targetId": 7 },
                    { "type": "Produce", "buildingId": 3, "unitType": "Worker", "count": 1 }
                ]
            })))
            .respond_with(ok(json!({
                "accepted": 1, "rejected": 1, "queueSize": 1, "queueCapacity": 20,
                "results": [
                    { "index": 0, "accepted": true, "commandId": 41, "error": null },
                    { "index": 1, "accepted": false, "commandId": null,
                      "error": { "code": "NOT_ENOUGH_RESOURCES", "message": "Need 50 ore" } }
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let commands = [
            CommandRequest::gather(vec![1], 7),
            CommandRequest::produce(3, UnitType::Worker, 1),
        ];
        let reply = api(&server).submit_commands("m1", &commands).await.unwrap();
        assert_eq!((reply.accepted, reply.rejected), (1, 1));
        assert_eq!(reply.results[0].command_id, Some(41));
        assert_eq!(reply.results[1].error.as_ref().unwrap().code, "NOT_ENOUGH_RESOURCES");
    }

    #[tokio::test]
    async fn get_result_parses_outcome_and_scores() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/matches/m1/result"))
            .respond_with(ok(json!({
                "matchId": "m1",
                "outcome": { "reason": "Elimination", "ticks": 812, "winnerId": "p1" },
                "players": [
                    player_result_json("p1", "Flaud", 0, true),
                    player_result_json("p2", "sitter", 1, false)
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let result = api(&server).get_result("m1").await.unwrap();
        assert_eq!(result.outcome.ticks, 812);
        assert_eq!(result.outcome.reason, MatchEndReason::Elimination);
        assert_eq!(result.outcome.winner_id.as_deref(), Some("p1"));
        assert!(result.players[0].winner);
        assert_eq!(result.players[0].score.economy, 1200);
        assert_eq!(result.players[1].units_killed, 9);
    }

    #[tokio::test]
    async fn get_result_before_end_is_an_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/matches/m1/result"))
            .respond_with(error(409, "MATCH_NOT_FINISHED", "Still running"))
            .mount(&server)
            .await;

        let err = api(&server).get_result("m1").await.unwrap_err();
        assert_eq!(err.code(), Some("MATCH_NOT_FINISHED"));
    }
}