use reqwest::{Client, RequestBuilder, Url};
use serde::de::DeserializeOwned;
use crate::bots::models::*;

/// HTTP client for the NetRts server API.
///
/// Cheap to clone: the inner [`Client`] is reference-counted, so clones share
/// one connection pool.
#[derive(Clone)]
pub struct Api {
    /// Name of the selected bot
    pub bot_name: String,
    /// Base URL of the server, e.g. `http://localhost:8080/`.
    pub server: Url,
    /// Player API key, sent with every authenticated request.
    pub api_key: String,
    /// Session token for the live game connection (set by `connect`).
    server_tok: Option<String>,
    client: Client,
}

impl Api {
    /// Sets the server token for the current Api instance, use after calling register_bot()
    /// 
    /// # Example
    /// 
    /// ```
    /// let api = Api::new("http://localhost:8080", "test-key");
    /// let player = api.register_bot("Flaud").await;
    /// ```
    pub fn set_token(&mut self, tok: String) {
        self.server_tok = Some(tok)
    }
    /// Creates a client for the server at `server_addr`, authenticating with `key`.
    ///
    /// # Panics
    ///
    /// Panics if `server_addr` is not a valid URL.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use netrts_bot::bots::api::Api;
    /// let api = Api::new("http://localhost:8080", "my-api-key");
    /// ```
    pub fn new<S, K>(name: String, server_addr: S, key: K) -> Self
    where
        S: AsRef<str>,
        K: Into<String>,
    {
        Self {
            bot_name: name,
            server: Url::parse(server_addr.as_ref()).expect("Invalid url"),
            api_key: key.into(),
            server_tok: None,
            client: Client::new(),
        }
    }

    /// Resolves `path` against the base server URL.
    ///
    /// `path` should be relative (no leading `/`). Note that [`Url::join`]
    /// replaces the last segment of a base path without a trailing slash:
    /// with base `http://host/rts`, `"api/v1"` becomes `http://host/api/v1`.
    ///
    /// # Panics
    ///
    /// Panics if `path` cannot be joined onto the base URL.
    fn url(&self, path: &str) -> Url {
        self.server.join(path).expect("Valid endpoint path")
    }

    /// Registers a new player named `name` (`POST /api/v1/players`).
    ///
    /// The returned [`RegisterPlayerResponse`] contains the player's API key.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, the server responds with a
    /// non-success status, or the response body can't be parsed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use netrts_bot::bots::api::Api;
    /// # async fn run() -> reqwest::Result<()> {
    /// let api = Api::new("http://localhost:8080", "");
    /// let player = api.register_bot("TestBot").await?;
    /// println!("Your API key: {}", player.api_key);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_bot(&self, name: impl Into<String>) -> reqwest::Result<RegisterPlayerResponse> {
        let payload = RegisterPlayerRequest { name: name.into() };
        self.send(self.client.post(self.url("api/v1/players")).json(&payload)).await
    }

    /// Creates a new match (`POST /api/v1/matches`).
    ///
    /// Use [`CreateMatchRequest::house_bots`] to fill slots with built-in
    /// opponents, or [`CreateMatchRequest::max_players`] to leave slots open
    /// for other bots to [`join_match`](Self::join_match).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, the server rejects the settings,
    /// or the response body can't be parsed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use netrts_bot::bots::{api::Api, models::*};
    /// # async fn run(api: Api) -> reqwest::Result<()> {
    /// let request = CreateMatchRequest {
    ///     house_bots: Some(vec!["balanced".into(), "rusher".into()]),
    ///     settings: Some(MatchSettingsDto {
    ///         seed: Some(1234),
    ///         max_ticks: Some(2000),
    ///         ..Default::default()
    ///     }),
    ///     ..Default::default()
    /// };
    /// let summary = api.create_match(&request).await?;
    /// println!("Created match {}", summary.match_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_match(&self, body: &CreateMatchRequest) -> reqwest::Result<MatchSummaryDto> {
        self.send(self.client.post(self.url("api/v1/matches")).json(body)).await
    }

    /// Joins the open match `match_id` (`POST /api/v1/matches/{id}/join`).
    ///
    /// # Errors
    ///
    /// Returns an error if the match doesn't exist, is already full or
    /// started, or the response body can't be parsed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use netrts_bot::bots::api::Api;
    /// # async fn run(api: Api) -> reqwest::Result<()> {
    /// let summary = api.join_match("abc123").await?;
    /// println!("Joined in slot {}", summary.players.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn join_match(&self, match_id: &str) -> reqwest::Result<MatchSummaryDto> {
        self.send(self.client.post(self.url(&format!("api/v1/matches/{match_id}/join")))).await
    }

    /// Attaches the API key, sends `req`, and deserializes a successful JSON response.
    async fn send<T: DeserializeOwned>(&self, req: RequestBuilder) -> reqwest::Result<T> {
        req.header("X-Api-Key", &self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn summary_json(id: &str) -> serde_json::Value {
        json!({
            "matchId": id,
            "createdAt": "2026-10-11T00:00:00Z",
            "status": "Waiting",
            "mapWidth": 64, "mapHeight": 64,
            "maxPlayers": 2, "maxTicks": 2000,
            "seed": 1234, "tick": 0, "tickIntervalMs": 100,
            "outcome": null,
            "players": []
        })
    }

    #[tokio::test]
    async fn create_match_sends_house_bot_request() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/matches"))
            .and(header("X-Api-Key", "test-key"))
            .and(body_json(json!({
                "houseBots": ["rusher"],
                "settings": { "seed": 1234 }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(summary_json("m1")))
            .expect(1) // fails the test if not called exactly once
            .mount(&server)
            .await;

        let api = Api::new("Flaud".to_string(), server.uri(), "test-key");
        let req = CreateMatchRequest {
            house_bots: Some(vec!["rusher".into()]),
            settings: Some(MatchSettingsDto { seed: Some(1234), ..Default::default() }),
            ..Default::default()
        };

        let summary = api.create_match(&req).await.unwrap();
        assert_eq!(summary.match_id, "m1");
    }

    #[tokio::test]
    async fn join_match_returns_error_on_404() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/matches/nope/join"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "code": "match_not_found", "message": "No such match"
            })))
            .mount(&server)
            .await;

        let api = Api::new("Flaude".to_string(), server.uri(), "test-key");
        let err = api.join_match("nope").await.unwrap_err();
        assert_eq!(err.status(), Some(reqwest::StatusCode::NOT_FOUND));
    }
}