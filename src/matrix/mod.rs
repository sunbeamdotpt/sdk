//! Matrix — chat and collaboration API client (Client-Server API).
//!
//! Build a shared [`sunbeam_g2v::client::Client`] (e.g. with
//! `ClientBuilder::new(url).auth(BearerToken::new(access_token))`) and pass it
//! to [`MatrixClient::new`], or use [`MatrixClient::connect`] for an
//! unauthenticated client derived from the active domain.

#[allow(missing_docs)]
pub mod types;

use http::Method;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sunbeam_g2v::client::{Client, ClientBuilder, RestClient};
use types::*;

use crate::error::{Result, SunbeamError};

/// Client for the Matrix Client-Server API.
pub struct MatrixClient {
    client: Client,
}

impl MatrixClient {
    /// Wrap a g2v client configured against the Matrix `/_matrix` base URL.
    ///
    /// If the base URL contains a path, it must end with a trailing slash —
    /// otherwise the last segment is dropped when request paths are joined.
    pub fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Build an unauthenticated client from domain
    /// (e.g. `https://messages.{domain}/_matrix`).
    pub fn connect(domain: &str) -> Result<Self> {
        let client = ClientBuilder::new(format!("https://messages.{domain}/_matrix/"))
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

    // -----------------------------------------------------------------------
    // Auth
    // -----------------------------------------------------------------------

    /// List supported login types.
    pub async fn list_login_types(&self) -> Result<LoginTypesResponse> {
        self.request_json(
            Method::GET,
            "client/v3/login",
            None::<&serde_json::Value>,
            "matrix list login types",
        )
        .await
    }

    /// Authenticate and obtain an access token.
    pub async fn login(&self, body: &LoginRequest) -> Result<LoginResponse> {
        self.request_json(Method::POST, "client/v3/login", Some(body), "matrix login")
            .await
    }

    /// Refresh an access token.
    pub async fn refresh(&self, body: &RefreshRequest) -> Result<RefreshResponse> {
        self.request_json(
            Method::POST,
            "client/v3/refresh",
            Some(body),
            "matrix refresh",
        )
        .await
    }

    /// Invalidate the current access token.
    pub async fn logout(&self) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/logout",
            None::<&serde_json::Value>,
            "matrix logout",
        )
        .await
    }

    /// Invalidate all access tokens for the user.
    pub async fn logout_all(&self) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/logout/all",
            None::<&serde_json::Value>,
            "matrix logout all",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Account
    // -----------------------------------------------------------------------

    /// Register a new account.
    pub async fn register(&self, body: &RegisterRequest) -> Result<RegisterResponse> {
        self.request_json(
            Method::POST,
            "client/v3/register",
            Some(body),
            "matrix register",
        )
        .await
    }

    /// Get the authenticated user's identity.
    pub async fn whoami(&self) -> Result<WhoamiResponse> {
        self.request_json(
            Method::GET,
            "client/v3/account/whoami",
            None::<&serde_json::Value>,
            "matrix whoami",
        )
        .await
    }

    /// List third-party identifiers for the account.
    pub async fn list_3pids(&self) -> Result<ThirdPartyIds> {
        self.request_json(
            Method::GET,
            "client/v3/account/3pid",
            None::<&serde_json::Value>,
            "matrix list 3pids",
        )
        .await
    }

    /// Add a third-party identifier to the account.
    pub async fn add_3pid(&self, body: &Add3pidRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/account/3pid/add",
            Some(body),
            "matrix add 3pid",
        )
        .await
    }

    /// Remove a third-party identifier from the account.
    pub async fn delete_3pid(&self, body: &Delete3pidRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/account/3pid/delete",
            Some(body),
            "matrix delete 3pid",
        )
        .await
    }

    /// Change the account password.
    pub async fn change_password(&self, body: &ChangePasswordRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/account/password",
            Some(body),
            "matrix change password",
        )
        .await
    }

    /// Deactivate the account.
    pub async fn deactivate(&self, body: &DeactivateRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/account/deactivate",
            Some(body),
            "matrix deactivate",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Rooms
    // -----------------------------------------------------------------------

    /// Create a new room.
    pub async fn create_room(&self, body: &CreateRoomRequest) -> Result<CreateRoomResponse> {
        self.request_json(
            Method::POST,
            "client/v3/createRoom",
            Some(body),
            "matrix create room",
        )
        .await
    }

    /// List public rooms on the server.
    pub async fn list_public_rooms(
        &self,
        limit: Option<u32>,
        since: Option<&str>,
    ) -> Result<PublicRoomsResponse> {
        let mut path = "client/v3/publicRooms".to_string();
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={l}"));
        }
        if let Some(s) = since {
            params.push(format!("since={s}"));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        self.request_json(
            Method::GET,
            &path,
            None::<&serde_json::Value>,
            "matrix list public rooms",
        )
        .await
    }

    /// Search public rooms with filtering.
    pub async fn search_public_rooms(
        &self,
        body: &SearchPublicRoomsRequest,
    ) -> Result<PublicRoomsResponse> {
        self.request_json(
            Method::POST,
            "client/v3/publicRooms",
            Some(body),
            "matrix search public rooms",
        )
        .await
    }

    /// Get a room's visibility in the directory.
    pub async fn get_room_visibility(&self, room_id: &str) -> Result<RoomVisibility> {
        self.request_json(
            Method::GET,
            &format!("client/v3/directory/list/room/{room_id}"),
            None::<&serde_json::Value>,
            "matrix get room visibility",
        )
        .await
    }

    /// Set a room's visibility in the directory.
    pub async fn set_room_visibility(
        &self,
        room_id: &str,
        body: &SetRoomVisibilityRequest,
    ) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/directory/list/room/{room_id}"),
            Some(body),
            "matrix set room visibility",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Membership
    // -----------------------------------------------------------------------

    /// Join a room by room ID.
    pub async fn join_room_by_id(&self, room_id: &str) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/join/{room_id}"),
            None::<&serde_json::Value>,
            "matrix join room by id",
        )
        .await
    }

    /// Join a room by alias.
    pub async fn join_room_by_alias(&self, alias: &str) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/join/{alias}"),
            None::<&serde_json::Value>,
            "matrix join room by alias",
        )
        .await
    }

    /// Leave a room.
    pub async fn leave_room(&self, room_id: &str) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/rooms/{room_id}/leave"),
            None::<&serde_json::Value>,
            "matrix leave room",
        )
        .await
    }

    /// Invite a user to a room.
    pub async fn invite(&self, room_id: &str, body: &InviteRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/rooms/{room_id}/invite"),
            Some(body),
            "matrix invite",
        )
        .await
    }

    /// Ban a user from a room.
    pub async fn ban(&self, room_id: &str, body: &BanRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/rooms/{room_id}/ban"),
            Some(body),
            "matrix ban",
        )
        .await
    }

    /// Unban a user from a room.
    pub async fn unban(&self, room_id: &str, body: &UnbanRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/rooms/{room_id}/unban"),
            Some(body),
            "matrix unban",
        )
        .await
    }

    /// Kick a user from a room.
    pub async fn kick(&self, room_id: &str, body: &KickRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/rooms/{room_id}/kick"),
            Some(body),
            "matrix kick",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // State
    // -----------------------------------------------------------------------

    /// Get all state events for a room.
    pub async fn get_all_state(&self, room_id: &str) -> Result<Vec<StateEvent>> {
        self.request_json(
            Method::GET,
            &format!("client/v3/rooms/{room_id}/state"),
            None::<&serde_json::Value>,
            "matrix get all state",
        )
        .await
    }

    /// Get a specific state event.
    pub async fn get_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
    ) -> Result<serde_json::Value> {
        self.request_json(
            Method::GET,
            &format!("client/v3/rooms/{room_id}/state/{event_type}/{state_key}"),
            None::<&serde_json::Value>,
            "matrix get state event",
        )
        .await
    }

    /// Set a state event in a room.
    pub async fn set_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        body: &serde_json::Value,
    ) -> Result<EventIdResponse> {
        self.request_json(
            Method::PUT,
            &format!("client/v3/rooms/{room_id}/state/{event_type}/{state_key}"),
            Some(body),
            "matrix set state event",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Messages
    // -----------------------------------------------------------------------

    /// Synchronise the client's state with the server.
    pub async fn sync(&self, params: &SyncParams) -> Result<SyncResponse> {
        let mut path = "client/v3/sync".to_string();
        let mut qs = Vec::new();
        if let Some(ref f) = params.filter {
            qs.push(format!("filter={f}"));
        }
        if let Some(ref s) = params.since {
            qs.push(format!("since={s}"));
        }
        if let Some(fs) = params.full_state {
            qs.push(format!("full_state={fs}"));
        }
        if let Some(ref sp) = params.set_presence {
            qs.push(format!("set_presence={sp}"));
        }
        if let Some(t) = params.timeout {
            qs.push(format!("timeout={t}"));
        }
        if !qs.is_empty() {
            path.push('?');
            path.push_str(&qs.join("&"));
        }
        self.request_json(
            Method::GET,
            &path,
            None::<&serde_json::Value>,
            "matrix sync",
        )
        .await
    }

    /// Send a message event to a room.
    pub async fn send_event(
        &self,
        room_id: &str,
        event_type: &str,
        txn_id: &str,
        body: &serde_json::Value,
    ) -> Result<EventIdResponse> {
        self.request_json(
            Method::PUT,
            &format!("client/v3/rooms/{room_id}/send/{event_type}/{txn_id}"),
            Some(body),
            "matrix send event",
        )
        .await
    }

    /// Get messages for a room.
    pub async fn get_messages(
        &self,
        room_id: &str,
        params: &MessagesParams,
    ) -> Result<MessagesResponse> {
        let mut path = format!("client/v3/rooms/{room_id}/messages?dir={}", params.dir);
        if let Some(ref f) = params.from {
            path.push_str(&format!("&from={f}"));
        }
        if let Some(ref t) = params.to {
            path.push_str(&format!("&to={t}"));
        }
        if let Some(l) = params.limit {
            path.push_str(&format!("&limit={l}"));
        }
        if let Some(ref f) = params.filter {
            path.push_str(&format!("&filter={f}"));
        }
        self.request_json(
            Method::GET,
            &path,
            None::<&serde_json::Value>,
            "matrix get messages",
        )
        .await
    }

    /// Get a single event from a room.
    pub async fn get_event(&self, room_id: &str, event_id: &str) -> Result<Event> {
        self.request_json(
            Method::GET,
            &format!("client/v3/rooms/{room_id}/event/{event_id}"),
            None::<&serde_json::Value>,
            "matrix get event",
        )
        .await
    }

    /// Get events around a given event.
    pub async fn get_context(
        &self,
        room_id: &str,
        event_id: &str,
        limit: Option<u32>,
    ) -> Result<ContextResponse> {
        let mut path = format!("client/v3/rooms/{room_id}/context/{event_id}");
        if let Some(l) = limit {
            path.push_str(&format!("?limit={l}"));
        }
        self.request_json(
            Method::GET,
            &path,
            None::<&serde_json::Value>,
            "matrix get context",
        )
        .await
    }

    /// Redact an event in a room.
    pub async fn redact(
        &self,
        room_id: &str,
        event_id: &str,
        txn_id: &str,
        body: &RedactRequest,
    ) -> Result<EventIdResponse> {
        self.request_json(
            Method::PUT,
            &format!("client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}"),
            Some(body),
            "matrix redact",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Presence
    // -----------------------------------------------------------------------

    /// Get presence status for a user.
    pub async fn get_presence(&self, user_id: &str) -> Result<PresenceStatus> {
        self.request_json(
            Method::GET,
            &format!("client/v3/presence/{user_id}/status"),
            None::<&serde_json::Value>,
            "matrix get presence",
        )
        .await
    }

    /// Set presence status for a user.
    pub async fn set_presence(&self, user_id: &str, body: &SetPresenceRequest) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/presence/{user_id}/status"),
            Some(body),
            "matrix set presence",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Typing
    // -----------------------------------------------------------------------

    /// Send a typing notification.
    pub async fn send_typing(
        &self,
        room_id: &str,
        user_id: &str,
        body: &TypingRequest,
    ) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/rooms/{room_id}/typing/{user_id}"),
            Some(body),
            "matrix send typing",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Receipts
    // -----------------------------------------------------------------------

    /// Send a read receipt.
    pub async fn send_receipt(
        &self,
        room_id: &str,
        receipt_type: &str,
        event_id: &str,
        body: &ReceiptRequest,
    ) -> Result<()> {
        self.request_send(
            Method::POST,
            &format!("client/v3/rooms/{room_id}/receipt/{receipt_type}/{event_id}"),
            Some(body),
            "matrix send receipt",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Profiles
    // -----------------------------------------------------------------------

    /// Get a user's profile.
    pub async fn get_profile(&self, user_id: &str) -> Result<Profile> {
        self.request_json(
            Method::GET,
            &format!("client/v3/profile/{user_id}"),
            None::<&serde_json::Value>,
            "matrix get profile",
        )
        .await
    }

    /// Get a user's display name.
    pub async fn get_displayname(&self, user_id: &str) -> Result<Displayname> {
        self.request_json(
            Method::GET,
            &format!("client/v3/profile/{user_id}/displayname"),
            None::<&serde_json::Value>,
            "matrix get displayname",
        )
        .await
    }

    /// Set a user's display name.
    pub async fn set_displayname(&self, user_id: &str, body: &SetDisplaynameRequest) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/profile/{user_id}/displayname"),
            Some(body),
            "matrix set displayname",
        )
        .await
    }

    /// Get a user's avatar URL.
    pub async fn get_avatar_url(&self, user_id: &str) -> Result<AvatarUrl> {
        self.request_json(
            Method::GET,
            &format!("client/v3/profile/{user_id}/avatar_url"),
            None::<&serde_json::Value>,
            "matrix get avatar url",
        )
        .await
    }

    /// Set a user's avatar URL.
    pub async fn set_avatar_url(&self, user_id: &str, body: &SetAvatarUrlRequest) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/profile/{user_id}/avatar_url"),
            Some(body),
            "matrix set avatar url",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Aliases
    // -----------------------------------------------------------------------

    /// Create a room alias.
    pub async fn create_alias(&self, alias: &str, body: &CreateAliasRequest) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/directory/room/{alias}"),
            Some(body),
            "matrix create alias",
        )
        .await
    }

    /// Resolve a room alias to a room ID.
    pub async fn resolve_alias(&self, alias: &str) -> Result<AliasResponse> {
        self.request_json(
            Method::GET,
            &format!("client/v3/directory/room/{alias}"),
            None::<&serde_json::Value>,
            "matrix resolve alias",
        )
        .await
    }

    /// Delete a room alias.
    pub async fn delete_alias(&self, alias: &str) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("client/v3/directory/room/{alias}"),
            None::<&serde_json::Value>,
            "matrix delete alias",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // User Directory
    // -----------------------------------------------------------------------

    /// Search the user directory.
    pub async fn search_users(&self, body: &UserSearchRequest) -> Result<UserSearchResponse> {
        self.request_json(
            Method::POST,
            "client/v3/user_directory/search",
            Some(body),
            "matrix search users",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Media
    // -----------------------------------------------------------------------

    /// Upload media content.
    pub async fn upload_media(&self, content_type: &str, data: Vec<u8>) -> Result<UploadResponse> {
        let resp = self
            .rest()
            .request(Method::POST, "media/v3/upload")?
            .header(http::header::CONTENT_TYPE, content_type)?
            .body(data)
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.into_body();
        if !status.is_success() {
            return Err(SunbeamError::network(format!(
                "matrix upload media: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        serde_json::from_slice(&bytes).map_err(|e| {
            SunbeamError::network(format!(
                "matrix upload media: failed to parse response: {e}"
            ))
        })
    }

    /// Download media content.
    pub async fn download_media(&self, server: &str, media_id: &str) -> Result<bytes::Bytes> {
        self.get_bytes(
            &format!("media/v3/download/{server}/{media_id}"),
            "matrix download media",
        )
        .await
    }

    /// Download a thumbnail of media content.
    pub async fn thumbnail(
        &self,
        server: &str,
        media_id: &str,
        params: &ThumbnailParams,
    ) -> Result<bytes::Bytes> {
        let mut path = format!(
            "media/v3/thumbnail/{server}/{media_id}?width={}&height={}",
            params.width, params.height
        );
        if let Some(ref m) = params.method {
            path.push_str(&format!("&method={m}"));
        }
        self.get_bytes(&path, "matrix thumbnail").await
    }

    // -----------------------------------------------------------------------
    // Devices
    // -----------------------------------------------------------------------

    /// List all devices for the authenticated user.
    pub async fn list_devices(&self) -> Result<DevicesResponse> {
        self.request_json(
            Method::GET,
            "client/v3/devices",
            None::<&serde_json::Value>,
            "matrix list devices",
        )
        .await
    }

    /// Get information about a specific device.
    pub async fn get_device(&self, device_id: &str) -> Result<Device> {
        self.request_json(
            Method::GET,
            &format!("client/v3/devices/{device_id}"),
            None::<&serde_json::Value>,
            "matrix get device",
        )
        .await
    }

    /// Update a device's metadata.
    pub async fn update_device(&self, device_id: &str, body: &UpdateDeviceRequest) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/devices/{device_id}"),
            Some(body),
            "matrix update device",
        )
        .await
    }

    /// Delete a device.
    pub async fn delete_device(&self, device_id: &str, body: &DeleteDeviceRequest) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("client/v3/devices/{device_id}"),
            Some(body),
            "matrix delete device",
        )
        .await
    }

    /// Delete multiple devices at once.
    pub async fn batch_delete_devices(&self, body: &BatchDeleteDevicesRequest) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/delete_devices",
            Some(body),
            "matrix batch delete devices",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // E2EE / Keys
    // -----------------------------------------------------------------------

    /// Upload end-to-end encryption keys.
    pub async fn upload_keys(&self, body: &KeysUploadRequest) -> Result<KeysUploadResponse> {
        self.request_json(
            Method::POST,
            "client/v3/keys/upload",
            Some(body),
            "matrix upload keys",
        )
        .await
    }

    /// Query users' device keys.
    pub async fn query_keys(&self, body: &KeysQueryRequest) -> Result<KeysQueryResponse> {
        self.request_json(
            Method::POST,
            "client/v3/keys/query",
            Some(body),
            "matrix query keys",
        )
        .await
    }

    /// Claim one-time keys.
    pub async fn claim_keys(&self, body: &KeysClaimRequest) -> Result<KeysClaimResponse> {
        self.request_json(
            Method::POST,
            "client/v3/keys/claim",
            Some(body),
            "matrix claim keys",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Push
    // -----------------------------------------------------------------------

    /// List pushers for the authenticated user.
    pub async fn list_pushers(&self) -> Result<PushersResponse> {
        self.request_json(
            Method::GET,
            "client/v3/pushers",
            None::<&serde_json::Value>,
            "matrix list pushers",
        )
        .await
    }

    /// Set a pusher for the authenticated user.
    pub async fn set_pusher(&self, body: &serde_json::Value) -> Result<()> {
        self.request_send(
            Method::POST,
            "client/v3/pushers/set",
            Some(body),
            "matrix set pusher",
        )
        .await
    }

    /// Get all push rules for the authenticated user.
    pub async fn get_push_rules(&self) -> Result<PushRulesResponse> {
        self.request_json(
            Method::GET,
            "client/v3/pushrules/",
            None::<&serde_json::Value>,
            "matrix get push rules",
        )
        .await
    }

    /// Set a push rule.
    pub async fn set_push_rule(
        &self,
        scope: &str,
        kind: &str,
        rule_id: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/pushrules/{scope}/{kind}/{rule_id}"),
            Some(body),
            "matrix set push rule",
        )
        .await
    }

    /// Delete a push rule.
    pub async fn delete_push_rule(&self, scope: &str, kind: &str, rule_id: &str) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("client/v3/pushrules/{scope}/{kind}/{rule_id}"),
            None::<&serde_json::Value>,
            "matrix delete push rule",
        )
        .await
    }

    /// Get notifications for the authenticated user.
    pub async fn get_notifications(
        &self,
        params: &NotificationsParams,
    ) -> Result<NotificationsResponse> {
        let mut path = "client/v3/notifications".to_string();
        let mut qs = Vec::new();
        if let Some(ref f) = params.from {
            qs.push(format!("from={f}"));
        }
        if let Some(l) = params.limit {
            qs.push(format!("limit={l}"));
        }
        if let Some(ref o) = params.only {
            qs.push(format!("only={o}"));
        }
        if !qs.is_empty() {
            path.push('?');
            path.push_str(&qs.join("&"));
        }
        self.request_json(
            Method::GET,
            &path,
            None::<&serde_json::Value>,
            "matrix get notifications",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Account Data
    // -----------------------------------------------------------------------

    /// Get account data for a user.
    pub async fn get_account_data(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> Result<serde_json::Value> {
        self.request_json(
            Method::GET,
            &format!("client/v3/user/{user_id}/account_data/{data_type}"),
            None::<&serde_json::Value>,
            "matrix get account data",
        )
        .await
    }

    /// Set account data for a user.
    pub async fn set_account_data(
        &self,
        user_id: &str,
        data_type: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/user/{user_id}/account_data/{data_type}"),
            Some(body),
            "matrix set account data",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Tags
    // -----------------------------------------------------------------------

    /// Get tags for a room.
    pub async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<TagsResponse> {
        self.request_json(
            Method::GET,
            &format!("client/v3/user/{user_id}/rooms/{room_id}/tags"),
            None::<&serde_json::Value>,
            "matrix get tags",
        )
        .await
    }

    /// Set a tag on a room.
    pub async fn set_tag(
        &self,
        user_id: &str,
        room_id: &str,
        tag: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}"),
            Some(body),
            "matrix set tag",
        )
        .await
    }

    /// Delete a tag from a room.
    pub async fn delete_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}"),
            None::<&serde_json::Value>,
            "matrix delete tag",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Search
    // -----------------------------------------------------------------------

    /// Search for messages in rooms.
    pub async fn search_messages(&self, body: &SearchRequest) -> Result<SearchResponse> {
        self.request_json(
            Method::POST,
            "client/v3/search",
            Some(body),
            "matrix search messages",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Filters
    // -----------------------------------------------------------------------

    /// Create a filter for a user.
    pub async fn create_filter(
        &self,
        user_id: &str,
        body: &serde_json::Value,
    ) -> Result<FilterIdResponse> {
        self.request_json(
            Method::POST,
            &format!("client/v3/user/{user_id}/filter"),
            Some(body),
            "matrix create filter",
        )
        .await
    }

    /// Get a previously created filter.
    pub async fn get_filter(&self, user_id: &str, filter_id: &str) -> Result<Filter> {
        self.request_json(
            Method::GET,
            &format!("client/v3/user/{user_id}/filter/{filter_id}"),
            None::<&serde_json::Value>,
            "matrix get filter",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Spaces
    // -----------------------------------------------------------------------

    /// Get the space hierarchy for a room.
    pub async fn get_space_hierarchy(
        &self,
        room_id: &str,
        params: &SpaceHierarchyParams,
    ) -> Result<SpaceHierarchy> {
        let mut path = format!("client/v1/rooms/{room_id}/hierarchy");
        let mut qs = Vec::new();
        if let Some(ref f) = params.from {
            qs.push(format!("from={f}"));
        }
        if let Some(l) = params.limit {
            qs.push(format!("limit={l}"));
        }
        if let Some(d) = params.max_depth {
            qs.push(format!("max_depth={d}"));
        }
        if let Some(s) = params.suggested_only {
            qs.push(format!("suggested_only={s}"));
        }
        if !qs.is_empty() {
            path.push('?');
            path.push_str(&qs.join("&"));
        }
        self.request_json(
            Method::GET,
            &path,
            None::<&serde_json::Value>,
            "matrix get space hierarchy",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Send-to-device
    // -----------------------------------------------------------------------

    /// Send an event to specific devices.
    pub async fn send_to_device(
        &self,
        event_type: &str,
        txn_id: &str,
        body: &SendToDeviceRequest,
    ) -> Result<()> {
        self.request_send(
            Method::PUT,
            &format!("client/v3/sendToDevice/{event_type}/{txn_id}"),
            Some(body),
            "matrix send to device",
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Send a request with an optional JSON body, error on non-2xx, parse the
    /// response as JSON.
    async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&(impl Serialize + Sync)>,
        ctx: &str,
    ) -> Result<T> {
        let mut req = self.rest().request(method, path)?;
        if let Some(b) = body {
            req = req
                .header(http::header::CONTENT_TYPE, "application/json")?
                .json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let bytes = resp.into_body();
        if !status.is_success() {
            return Err(SunbeamError::network(format!(
                "{ctx}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Send a request with an optional JSON body, error on non-2xx, discard
    /// the response body.
    async fn request_send(
        &self,
        method: Method,
        path: &str,
        body: Option<&(impl Serialize + Sync)>,
        ctx: &str,
    ) -> Result<()> {
        let mut req = self.rest().request(method, path)?;
        if let Some(b) = body {
            req = req
                .header(http::header::CONTENT_TYPE, "application/json")?
                .json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.into_body();
            return Err(SunbeamError::network(format!(
                "{ctx}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(())
    }

    /// GET a path and return the raw response bytes, erroring on non-2xx.
    async fn get_bytes(&self, path: &str, ctx: &str) -> Result<bytes::Bytes> {
        let resp = self.rest().request(Method::GET, path)?.send().await?;
        let status = resp.status();
        let bytes = resp.into_body();
        if !status.is_success() {
            return Err(SunbeamError::network(format!(
                "{ctx}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_url() {
        let c = MatrixClient::connect("sunbeam.pt").unwrap();
        assert_eq!(c.base_url(), "https://messages.sunbeam.pt/_matrix");
    }

    #[test]
    fn test_new_from_g2v_client() {
        let client = ClientBuilder::new("http://localhost:8008/_matrix")
            .build()
            .unwrap();
        let c = MatrixClient::new(&client);
        assert_eq!(c.base_url(), "http://localhost:8008/_matrix");
    }
}

#[cfg(all(test, feature = "testing"))]
mod container_tests {
    use std::time::Duration;

    use serde_json::json;
    use sunbeam_g2v::client::{BearerToken, ClientBuilder};

    use super::MatrixClient;
    use super::types::CreateRoomRequest;
    use crate::testing::Tuwunel;

    /// Registration token baked into the Tuwunel test config.
    const REGISTRATION_TOKEN: &str = "sunbeam-test-token";

    /// Boot a Tuwunel container, register a user, and return an authenticated
    /// client. Registration uses reqwest directly because the token flow may
    /// need a two-step session handshake that is not part of the client API.
    async fn boot() -> (
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
        MatrixClient,
    ) {
        let container = Tuwunel::default()
            .publish_ports()
            .start()
            .await
            .expect("tuwunel should start");
        let url = Tuwunel::url(&container).await.expect("url should resolve");

        // Wait for the client API.
        let http = reqwest::Client::new();
        let versions = format!("{url}/_matrix/client/versions");
        for _ in 0..30 {
            if let Ok(resp) = http.get(&versions).send().await
                && resp.status().is_success()
            {
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        // Register (two-step if the server asks for a session).
        let register_url = format!("{url}/_matrix/client/v3/register");
        let username = format!("sdk-test-{}", uuid());
        let body = json!({
            "username": username,
            "password": "sdk-test-password",
            "auth": {"type": "m.login.registration_token", "token": REGISTRATION_TOKEN},
        });
        let resp = http
            .post(&register_url)
            .json(&body)
            .send()
            .await
            .expect("register request");
        let value: serde_json::Value = if resp.status().is_success() {
            resp.json().await.expect("register body")
        } else {
            let err: serde_json::Value = resp.json().await.expect("register flow body");
            let session = err["session"].as_str().expect("session in 401 response");
            let body = json!({
                "username": username,
                "password": "sdk-test-password",
                "auth": {
                    "type": "m.login.registration_token",
                    "token": REGISTRATION_TOKEN,
                    "session": session,
                },
            });
            http.post(&register_url)
                .json(&body)
                .send()
                .await
                .expect("register retry")
                .json()
                .await
                .expect("register retry body")
        };
        let access_token = value["access_token"]
            .as_str()
            .expect("access_token in register response")
            .to_string();

        let g2v = ClientBuilder::new(format!("{url}/_matrix/"))
            .auth(BearerToken::new(access_token))
            .build()
            .expect("g2v client");
        (container, MatrixClient::new(&g2v))
    }

    fn uuid() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        format!(
            "{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    }

    #[tokio::test]
    async fn matrix_room_and_message_lifecycle() {
        let (_container, client) = boot().await;

        let whoami = client.whoami().await.expect("whoami");
        assert!(whoami.user_id.contains("sdk-test-"));

        let room = client
            .create_room(&CreateRoomRequest {
                name: Some("sdk-test-room".into()),
                ..Default::default()
            })
            .await
            .expect("create room");

        let event = client
            .send_event(
                &room.room_id,
                "m.room.message",
                "txn-1",
                &json!({"msgtype": "m.text", "body": "hello from sdk"}),
            )
            .await
            .expect("send event");
        assert!(!event.event_id.is_empty());

        let fetched = client
            .get_event(&room.room_id, &event.event_id)
            .await
            .expect("get event");
        assert_eq!(fetched.content["body"], "hello from sdk");

        client.leave_room(&room.room_id).await.expect("leave room");
    }
}
