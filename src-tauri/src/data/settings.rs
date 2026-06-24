use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

include!("settings/ini.rs");
include!("settings/store.rs");
include!("settings/theme.rs");
include!("settings/presets.rs");
include!("settings/paths.rs");
include!("settings/commands.rs");
