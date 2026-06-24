use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

include!("websocket/core.rs");
include!("websocket/server_impl.rs");
include!("websocket/connection.rs");
