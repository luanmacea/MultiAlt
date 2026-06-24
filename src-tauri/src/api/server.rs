use axum::{
    body::Body,
    extract::{Extension, Query, Request},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::sync::watch;

use crate::api::{auth, roblox};
use crate::data::accounts::AccountStore;
use crate::data::settings::SettingsStore;

include!("server/state.rs");
include!("server/launch_patch.rs");
include!("server/query.rs");
include!("server/helpers.rs");
include!("server/middleware.rs");
include!("server/handlers_basic.rs");
include!("server/handlers_launch.rs");
include!("server/handlers_edit.rs");
include!("server/route_wrappers.rs");
include!("server/runtime.rs");
