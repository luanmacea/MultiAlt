static NEXUS: LazyLock<NexusServer> = LazyLock::new(NexusServer::new);

pub fn nexus() -> &'static NexusServer {
    &NEXUS
}

pub struct NexusServer {
    connections: Mutex<HashMap<String, NexusConnection>>,
    accounts: Mutex<Vec<ControlledAccount>>,
    custom_elements: Mutex<Vec<CustomElement>>,
    log_messages: Mutex<Vec<String>>,
    server_handle: Mutex<Option<ServerHandle>>,
}

struct ServerHandle {
    shutdown: tokio::sync::watch::Sender<bool>,
    port: u16,
}

struct NexusConnection {
    sender: mpsc::UnboundedSender<String>,
    #[allow(dead_code)]
    username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlledAccount {
    #[serde(rename = "Username")]
    pub username: String,
    #[serde(rename = "AutoExecute", default)]
    pub auto_execute: String,
    #[serde(rename = "PlaceId", default)]
    pub place_id: i64,
    #[serde(rename = "JobId", default)]
    pub job_id: String,
    #[serde(rename = "RelaunchDelay", default = "default_relaunch_delay")]
    pub relaunch_delay: f64,
    #[serde(rename = "AutoRelaunch", default)]
    pub auto_relaunch: bool,
    #[serde(rename = "IsChecked", default)]
    pub is_checked: bool,
    #[serde(skip)]
    pub status: AccountStatus,
    #[serde(skip)]
    pub last_ping: Option<Instant>,
    #[serde(skip)]
    pub in_game_job_id: String,
    #[serde(skip)]
    pub client_can_receive: bool,
}

fn default_relaunch_delay() -> f64 {
    30.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AccountStatus {
    Online,
    Offline,
}

impl Default for AccountStatus {
    fn default() -> Self {
        AccountStatus::Offline
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomElement {
    pub name: String,
    pub element_type: String,
    pub content: String,
    pub size: Option<(i32, i32)>,
    pub margin: Option<(i32, i32, i32, i32)>,
    pub decimal_places: Option<i32>,
    pub increment: Option<String>,
    pub value: String,
    pub is_newline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub connected_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountView {
    pub username: String,
    pub auto_execute: String,
    pub place_id: i64,
    pub job_id: String,
    pub relaunch_delay: f64,
    pub auto_relaunch: bool,
    pub is_checked: bool,
    pub status: String,
    pub in_game_job_id: String,
}

#[derive(Deserialize)]
struct Command {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Payload")]
    payload: Option<HashMap<String, String>>,
}
