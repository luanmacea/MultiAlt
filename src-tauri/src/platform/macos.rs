use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::io::{Read, Seek, SeekFrom};
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

include!("macos/core.rs");
include!("macos/launch.rs");
include!("macos/client_settings.rs");
include!("macos/logs.rs");
include!("macos/tracker.rs");
