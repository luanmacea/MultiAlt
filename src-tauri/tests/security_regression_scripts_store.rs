#[path = "../src/data/scripts.rs"]
mod scripts;

use scripts::{ManagedScript, ScriptPermissions, ScriptStore};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_scripts_path() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("ram-script-security-probe-{}.json", now))
}

fn valid_script(id: &str) -> ManagedScript {
    ManagedScript {
        id: id.to_string(),
        name: "Security Probe".to_string(),
        description: "Probe".to_string(),
        language: "javascript".to_string(),
        source: "ram.info('ok');".to_string(),
        enabled: true,
        trusted: false,
        auto_start: false,
        permissions: ScriptPermissions::default(),
        created_at_ms: 0,
        updated_at_ms: 0,
    }
}

#[test]
fn scripts_store_rejects_invalid_ids_and_oversized_source() {
    let path = temp_scripts_path();
    let store = ScriptStore::new(path.clone());

    let mut invalid_id = valid_script("bad id with spaces");
    let invalid_id_result = store.upsert(invalid_id.clone());
    assert!(invalid_id_result.is_err(), "expected invalid id to fail");

    invalid_id.id = "valid-id".to_string();
    invalid_id.source = "x".repeat(300_000);
    let oversized_source_result = store.upsert(invalid_id);
    assert!(
        oversized_source_result.is_err(),
        "expected oversized source to fail"
    );

    let valid_result = store.upsert(valid_script("valid-id-2"));
    assert!(valid_result.is_ok(), "expected valid script to be accepted");

    let _ = fs::remove_file(path);
}
