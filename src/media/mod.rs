//! LiveKit — real-time media API client (Twirp).
//!
//! Build a shared [`sunbeam_g2v::client::Client`] (e.g. with
//! `ClientBuilder::new(url).auth(BearerToken::new(token))`) and pass it to
//! [`LiveKitClient::new`], or use [`LiveKitClient::connect`] for an
//! unauthenticated client derived from the active domain.

#[allow(missing_docs)]
pub mod types;

use base64::Engine;
use sunbeam_g2v::client::{Client, ClientBuilder, RestClient};
use types::*;

use crate::error::{Result, SunbeamError};

/// Client for the LiveKit Twirp API.
pub struct LiveKitClient {
    client: Client,
}

impl LiveKitClient {
    /// Wrap a g2v client configured against the LiveKit base URL.
    ///
    /// If the base URL contains a path, it must end with a trailing slash —
    /// otherwise the last segment is dropped when request paths are joined.
    pub fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Build an unauthenticated client from domain
    /// (e.g. `https://livekit.{domain}`).
    pub fn connect(domain: &str) -> Result<Self> {
        let client = ClientBuilder::new(format!("https://livekit.{domain}"))
            .build()
            .map_err(|e| SunbeamError::Other(e.to_string()))?;
        Ok(Self::new(&client))
    }

    /// The base URL this client is configured against.
    pub fn base_url(&self) -> &str {
        self.client.base_url().as_str().trim_end_matches('/')
    }

    fn rest(&self) -> RestClient {
        self.client.rest()
    }

    // -- Rooms ---------------------------------------------------------------

    /// Create a room.
    pub async fn create_room(&self, body: &(impl serde::Serialize + Sync)) -> Result<Room> {
        self.twirp("livekit.RoomService/CreateRoom", body).await
    }

    /// List all rooms.
    pub async fn list_rooms(&self) -> Result<ListRoomsResponse> {
        self.twirp("livekit.RoomService/ListRooms", &serde_json::json!({}))
            .await
    }

    /// Delete a room.
    pub async fn delete_room(&self, body: &(impl serde::Serialize + Sync)) -> Result<()> {
        self.twirp_send("livekit.RoomService/DeleteRoom", body)
            .await
    }

    /// Update room metadata.
    pub async fn update_room_metadata(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<Room> {
        self.twirp("livekit.RoomService/UpdateRoomMetadata", body)
            .await
    }

    /// Send data to a room.
    pub async fn send_data(&self, body: &(impl serde::Serialize + Sync)) -> Result<()> {
        self.twirp_send("livekit.RoomService/SendData", body).await
    }

    // -- Participants --------------------------------------------------------

    /// List participants in a room.
    pub async fn list_participants(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<ListParticipantsResponse> {
        self.twirp("livekit.RoomService/ListParticipants", body)
            .await
    }

    /// Get a single participant.
    pub async fn get_participant(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<ParticipantInfo> {
        self.twirp("livekit.RoomService/GetParticipant", body).await
    }

    /// Remove a participant from a room.
    pub async fn remove_participant(&self, body: &(impl serde::Serialize + Sync)) -> Result<()> {
        self.twirp_send("livekit.RoomService/RemoveParticipant", body)
            .await
    }

    /// Update a participant.
    pub async fn update_participant(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<ParticipantInfo> {
        self.twirp("livekit.RoomService/UpdateParticipant", body)
            .await
    }

    /// Mute a published track.
    pub async fn mute_track(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<MuteTrackResponse> {
        self.twirp("livekit.RoomService/MutePublishedTrack", body)
            .await
    }

    // -- Egress --------------------------------------------------------------

    /// Start a room composite egress.
    pub async fn start_room_composite_egress(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<EgressInfo> {
        self.twirp("livekit.Egress/StartRoomCompositeEgress", body)
            .await
    }

    /// Start a track egress.
    pub async fn start_track_egress(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<EgressInfo> {
        self.twirp("livekit.Egress/StartTrackEgress", body).await
    }

    /// List egress sessions.
    pub async fn list_egress(
        &self,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<ListEgressResponse> {
        self.twirp("livekit.Egress/ListEgress", body).await
    }

    /// Stop an egress session.
    pub async fn stop_egress(&self, body: &(impl serde::Serialize + Sync)) -> Result<EgressInfo> {
        self.twirp("livekit.Egress/StopEgress", body).await
    }

    // -- Token ---------------------------------------------------------------

    /// Generate a LiveKit access token (JWT signed with HMAC-SHA256).
    ///
    /// - `api_key`: LiveKit API key (used as `iss` claim).
    /// - `api_secret`: LiveKit API secret (HMAC key).
    /// - `identity`: participant identity (used as `sub` claim).
    /// - `grants`: video grant permissions.
    /// - `ttl_secs`: token lifetime in seconds.
    pub fn generate_access_token(
        api_key: &str,
        api_secret: &str,
        identity: &str,
        grants: &VideoGrants,
        ttl_secs: u64,
    ) -> Result<String> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;

        let header = serde_json::json!({"alg": "HS256", "typ": "JWT"});
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| SunbeamError::Other(format!("system time error: {e}")))?
            .as_secs();

        let claims = serde_json::json!({
            "iss": api_key,
            "sub": identity,
            "nbf": now,
            "exp": now + ttl_secs,
            "video": grants,
        });

        let header_b64 = b64.encode(serde_json::to_vec(&header)?);
        let claims_b64 = b64.encode(serde_json::to_vec(&claims)?);
        let signing_input = format!("{header_b64}.{claims_b64}");

        let mut mac = Hmac::<Sha256>::new_from_slice(api_secret.as_bytes())
            .map_err(|e| SunbeamError::Other(format!("HMAC key error: {e}")))?;
        mac.update(signing_input.as_bytes());
        let signature = b64.encode(mac.finalize().into_bytes());

        Ok(format!("{signing_input}.{signature}"))
    }

    // -- Internal helpers ----------------------------------------------------

    /// Twirp POST that returns a parsed JSON response.
    async fn twirp<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        body: &(impl serde::Serialize + Sync),
    ) -> Result<T> {
        let resp = self
            .rest()
            .post(&format!("twirp/{method}"))?
            .header(http::header::CONTENT_TYPE, "application/json")?
            .json(body)
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.into_body();
        if !status.is_success() {
            return Err(SunbeamError::network(format!(
                "{method}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Twirp POST that discards the response body.
    async fn twirp_send(&self, method: &str, body: &(impl serde::Serialize + Sync)) -> Result<()> {
        let resp = self
            .rest()
            .post(&format!("twirp/{method}"))?
            .header(http::header::CONTENT_TYPE, "application/json")?
            .json(body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.into_body();
            return Err(SunbeamError::network(format!(
                "{method}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_url() {
        let c = LiveKitClient::connect("sunbeam.pt").unwrap();
        assert_eq!(c.base_url(), "https://livekit.sunbeam.pt");
    }

    #[test]
    fn test_new_from_g2v_client() {
        let client = ClientBuilder::new("http://localhost:7880").build().unwrap();
        let c = LiveKitClient::new(&client);
        assert_eq!(c.base_url(), "http://localhost:7880");
    }

    #[test]
    fn test_generate_access_token() {
        let grants = VideoGrants {
            room_join: Some(true),
            room: Some("test-room".into()),
            can_publish: Some(true),
            can_subscribe: Some(true),
            ..Default::default()
        };
        let token =
            LiveKitClient::generate_access_token("api-key", "api-secret", "user-1", &grants, 3600)
                .unwrap();

        // JWT has three dot-separated parts
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        // Verify header
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header_bytes = b64.decode(parts[0]).unwrap();
        let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();
        assert_eq!(header["alg"], "HS256");
        assert_eq!(header["typ"], "JWT");

        // Verify claims
        let claims_bytes = b64.decode(parts[1]).unwrap();
        let claims: serde_json::Value = serde_json::from_slice(&claims_bytes).unwrap();
        assert_eq!(claims["iss"], "api-key");
        assert_eq!(claims["sub"], "user-1");
        assert!(claims["exp"].as_u64().unwrap() > claims["nbf"].as_u64().unwrap());
        assert_eq!(claims["video"]["roomJoin"], true);
        assert_eq!(claims["video"]["room"], "test-room");
    }

    #[test]
    fn test_generate_access_token_signature_valid() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let grants = VideoGrants {
            room_create: Some(true),
            ..Default::default()
        };
        let secret = "my-secret-key";
        let token =
            LiveKitClient::generate_access_token("key", secret, "id", &grants, 600).unwrap();

        let parts: Vec<&str> = token.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let sig_bytes = b64.decode(parts[2]).unwrap();

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signing_input.as_bytes());
        assert!(mac.verify_slice(&sig_bytes).is_ok());
    }

    #[tokio::test]
    async fn test_list_rooms_unreachable() {
        let client = ClientBuilder::new("http://127.0.0.1:19998")
            .build()
            .unwrap();
        let c = LiveKitClient::new(&client);
        let result = c.list_rooms().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_room_unreachable() {
        let client = ClientBuilder::new("http://127.0.0.1:19998")
            .build()
            .unwrap();
        let c = LiveKitClient::new(&client);
        let body = serde_json::json!({"name": "test-room"});
        let result = c.create_room(&body).await;
        assert!(result.is_err());
    }
}

#[cfg(all(test, feature = "testing"))]
mod container_tests {
    use std::time::Duration;

    use serde_json::json;
    use sunbeam_g2v::client::{BearerToken, ClientBuilder};

    use super::{LiveKitClient, types::VideoGrants};
    use crate::testing::LiveKit;

    /// Boot a LiveKit container and return a client authenticated with a
    /// server API token signed by the container's dev keys.
    async fn boot() -> (
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
        LiveKitClient,
    ) {
        let container = LiveKit::default()
            .publish_ports()
            .start()
            .await
            .expect("livekit should start");
        let url = LiveKit::url(&container).await.expect("url should resolve");

        let grants = VideoGrants {
            room_create: Some(true),
            room_list: Some(true),
            room_admin: Some(true),
            ..Default::default()
        };
        let token = LiveKitClient::generate_access_token(
            LiveKit::API_KEY,
            LiveKit::API_SECRET,
            "sdk-test",
            &grants,
            600,
        )
        .expect("token");

        let g2v = ClientBuilder::new(url)
            .auth(BearerToken::new(token))
            .build()
            .expect("g2v client");
        let client = LiveKitClient::new(&g2v);

        // Wait for the HTTP server to accept requests.
        for _ in 0..30 {
            if client.list_rooms().await.is_ok() {
                return (container, client);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        panic!("livekit did not become ready");
    }

    #[tokio::test]
    async fn livekit_room_lifecycle() {
        let (_container, client) = boot().await;

        let room = client
            .create_room(&json!({"name": "sdk-test-room"}))
            .await
            .expect("create room");
        assert_eq!(room.name, "sdk-test-room");

        let rooms = client.list_rooms().await.expect("list rooms");
        assert!(
            rooms.rooms.iter().any(|r| r.name == "sdk-test-room"),
            "created room should appear in list"
        );

        client
            .delete_room(&json!({"room": "sdk-test-room"}))
            .await
            .expect("delete room");

        let rooms = client.list_rooms().await.expect("list rooms after delete");
        assert!(
            !rooms.rooms.iter().any(|r| r.name == "sdk-test-room"),
            "deleted room should be gone"
        );
    }
}
