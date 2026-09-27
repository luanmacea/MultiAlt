use crate::api::endpoints;
use crate::api::http_client;
use reqwest::header::COOKIE;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};

include!("roblox/http.rs");
include!("roblox/users.rs");
include!("roblox/avatar_games.rs");
include!("roblox/private_links.rs");
include!("roblox/join_links.rs");
include!("roblox/social_presence.rs");
include!("roblox/friends.rs");
include!("roblox/server_regions.rs");
include!("roblox/server_pick.rs");
include!("roblox/username_check.rs");
include!("roblox/thumbnails.rs");
include!("roblox/economy.rs");
