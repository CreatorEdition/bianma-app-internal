use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use std::{collections::BTreeSet, sync::Mutex};

use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::error::AppError;
use crate::services::webdav_sync as webdav_sync_service;
use crate::settings::{self, WebDavSyncScope, WebDavSyncSettings};

const AUTO_SYNC_DEBOUNCE_MS: u64 = 1000;
pub(crate) const MAX_AUTO_SYNC_WAIT_MS: u64 = 10_000;

static DB_CHANGE_TX: OnceLock<Sender<String>> = OnceLock::new();
static PENDING_CHANGED_TABLES: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
static AUTO_SYNC_SUPPRESS_DEPTH: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct AutoSyncSuppressionGuard;

impl AutoSyncSuppressionGuard {
    pub fn new() -> Self {
        AUTO_SYNC_SUPPRESS_DEPTH.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for AutoSyncSuppressionGuard {
    fn drop(&mut self) {
        let _ =
            AUTO_SYNC_SUPPRESS_DEPTH.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                Some(value.saturating_sub(1))
            });
    }
}

pub(crate) fn is_auto_sync_suppressed() -> bool {
    AUTO_SYNC_SUPPRESS_DEPTH.load(Ordering::SeqCst) > 0
}

pub fn should_trigger_for_table(table: &str) -> bool {
    table_scope_name(table).is_some()
}

fn table_scope_name(table: &str) -> Option<&'static str> {
    let normalized = table.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "providers" | "provider_endpoints" => Some("providers"),
        "mcp_servers" => Some("mcp"),
        "prompts" => Some("prompts"),
        _ => None,
    }
}

fn pending_changed_tables() -> &'static Mutex<BTreeSet<String>> {
    PENDING_CHANGED_TABLES.get_or_init(|| Mutex::new(BTreeSet::new()))
}

fn record_changed_table(table: &str) {
    pending_changed_tables()
        .lock()
        .expect("pending changed tables mutex poisoned")
        .insert(table.to_string());
}

fn take_pending_changed_tables() -> Vec<String> {
    let mut pending = pending_changed_tables()
        .lock()
        .expect("pending changed tables mutex poisoned");
    let tables = pending.iter().cloned().collect();
    pending.clear();
    tables
}

pub(crate) fn enqueue_change_signal(tx: &Sender<String>, table: &str) -> bool {
    match tx.try_send(table.to_string()) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => false,
    }
}

pub(crate) fn auto_sync_wait_duration(started_at: Instant, now: Instant) -> Option<Duration> {
    let max_wait = Duration::from_millis(MAX_AUTO_SYNC_WAIT_MS);
    let debounce = Duration::from_millis(AUTO_SYNC_DEBOUNCE_MS);
    let elapsed = now.saturating_duration_since(started_at);
    if elapsed >= max_wait {
        return None;
    }
    Some(debounce.min(max_wait - elapsed))
}

fn should_run_auto_sync(settings: Option<&WebDavSyncSettings>) -> bool {
    let Some(sync) = settings else {
        return false;
    };
    sync.enabled && sync.auto_sync
}

fn scope_allows_table(scope: &WebDavSyncScope, table: &str) -> bool {
    match table_scope_name(table) {
        Some("providers") => scope.providers,
        Some("mcp") => scope.mcp,
        Some("prompts") => scope.prompts,
        _ => false,
    }
}

fn should_auto_sync_for_tables(
    settings: Option<&WebDavSyncSettings>,
    changed_tables: &[String],
) -> bool {
    let Some(sync) = settings else {
        return false;
    };
    sync.enabled
        && sync.auto_sync
        && changed_tables
            .iter()
            .any(|table| scope_allows_table(&sync.scope, table))
}

fn persist_auto_sync_error(settings: &mut WebDavSyncSettings, error: &AppError) {
    settings.status.last_error = Some(error.to_string());
    settings.status.last_error_source = Some("auto".to_string());
    let _ = settings::update_webdav_sync_status(settings.status.clone());
}

fn emit_auto_sync_status_updated(app: &AppHandle, status: &str, error: Option<&str>) {
    let payload = match error {
        Some(message) => json!({
            "source": "auto",
            "status": status,
            "error": message,
        }),
        None => json!({
            "source": "auto",
            "status": status,
        }),
    };

    if let Err(err) = app.emit("webdav-sync-status-updated", payload) {
        log::debug!("[WebDAV] failed to emit sync status update event: {err}");
    }
}

async fn run_auto_sync_upload(
    db: &crate::database::Database,
    app: &AppHandle,
    changed_tables: &[String],
) -> Result<(), AppError> {
    let mut settings = settings::get_webdav_sync_settings();
    if !should_run_auto_sync(settings.as_ref()) {
        return Ok(());
    }

    if !should_auto_sync_for_tables(settings.as_ref(), changed_tables) {
        log::debug!(
            "[WebDAV][AutoSync] Skipped because changed tables are outside selected scope: {:?}",
            changed_tables
        );
        return Ok(());
    }

    let mut sync_settings = match settings.take() {
        Some(value) => value,
        None => return Ok(()),
    };

    let result = webdav_sync_service::run_with_sync_lock(webdav_sync_service::upload(
        db,
        &mut sync_settings,
    ))
    .await;
    match result {
        Ok(_) => {
            emit_auto_sync_status_updated(app, "success", None);
            Ok(())
        }
        Err(err) => {
            persist_auto_sync_error(&mut sync_settings, &err);
            emit_auto_sync_status_updated(app, "error", Some(&err.to_string()));
            Err(err)
        }
    }
}

pub fn notify_db_changed(table: &str) {
    if is_auto_sync_suppressed() {
        return;
    }
    if !should_trigger_for_table(table) {
        return;
    }
    let Some(tx) = DB_CHANGE_TX.get() else {
        return;
    };
    record_changed_table(table);
    let _ = enqueue_change_signal(tx, table);
}

pub fn start_worker(db: Arc<crate::database::Database>, app: tauri::AppHandle) {
    if DB_CHANGE_TX.get().is_some() {
        return;
    }

    // Buffer size 1 is enough: we only need "dirty" signals, not every event.
    let (tx, rx) = channel::<String>(1);
    if DB_CHANGE_TX.set(tx).is_err() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        run_worker_loop(db, rx, app).await;
    });
}

async fn run_worker_loop(
    db: Arc<crate::database::Database>,
    mut rx: Receiver<String>,
    app: tauri::AppHandle,
) {
    while let Some(first_table) = rx.recv().await {
        let started_at = Instant::now();
        let mut merged_count = 1usize;

        loop {
            let Some(wait_for) = auto_sync_wait_duration(started_at, Instant::now()) else {
                break;
            };
            let timeout = tokio::time::timeout(wait_for, rx.recv()).await;

            match timeout {
                Ok(Some(table)) => {
                    merged_count += 1;
                    record_changed_table(&table);
                }
                Ok(None) => return,
                Err(_) => break,
            }
        }

        let changed_tables = take_pending_changed_tables();
        if changed_tables.is_empty() {
            log::debug!(
                "[WebDAV][AutoSync] Triggered by table={first_table}, merged_changes={merged_count}, but no pending tables remained"
            );
            continue;
        }

        log::debug!(
            "[WebDAV][AutoSync] Triggered by table={first_table}, merged_changes={merged_count}, changed_tables={:?}",
            changed_tables
        );

        if let Err(err) = run_auto_sync_upload(&db, &app, &changed_tables).await {
            log::warn!("[WebDAV][AutoSync] Upload failed: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        auto_sync_wait_duration, enqueue_change_signal, is_auto_sync_suppressed,
        record_changed_table, scope_allows_table, should_auto_sync_for_tables,
        should_run_auto_sync, should_trigger_for_table, take_pending_changed_tables,
        AutoSyncSuppressionGuard, MAX_AUTO_SYNC_WAIT_MS,
    };
    use crate::settings::{WebDavSyncScope, WebDavSyncSettings};
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc::channel;

    #[test]
    fn should_trigger_sync_for_config_tables_only() {
        assert!(should_trigger_for_table("providers"));
        assert!(should_trigger_for_table("provider_endpoints"));
        assert!(should_trigger_for_table("mcp_servers"));
        assert!(should_trigger_for_table("prompts"));
        assert!(!should_trigger_for_table("settings"));
        assert!(!should_trigger_for_table("proxy_config"));
        assert!(!should_trigger_for_table("skills"));
        assert!(!should_trigger_for_table("skill_repos"));
        assert!(!should_trigger_for_table("proxy_request_logs"));
        assert!(!should_trigger_for_table("provider_health"));
    }

    #[test]
    fn scope_allows_only_matching_tables() {
        let scope = WebDavSyncScope {
            providers: true,
            mcp: false,
            prompts: false,
        };
        assert!(scope_allows_table(&scope, "providers"));
        assert!(scope_allows_table(&scope, "provider_endpoints"));
        assert!(!scope_allows_table(&scope, "mcp_servers"));
        assert!(!scope_allows_table(&scope, "prompts"));
        assert!(!scope_allows_table(&scope, "settings"));
    }

    #[test]
    fn should_auto_sync_for_tables_requires_matching_scope() {
        let settings = WebDavSyncSettings {
            enabled: true,
            auto_sync: true,
            scope: WebDavSyncScope {
                providers: false,
                mcp: false,
                prompts: true,
            },
            ..WebDavSyncSettings::default()
        };

        assert!(should_auto_sync_for_tables(
            Some(&settings),
            &[String::from("prompts")]
        ));
        assert!(!should_auto_sync_for_tables(
            Some(&settings),
            &[String::from("mcp_servers")]
        ));
        assert!(!should_auto_sync_for_tables(
            Some(&settings),
            &[String::from("provider_endpoints")]
        ));
    }

    #[test]
    fn dropped_signal_still_retains_scope_matching_table() {
        let (tx, _rx) = channel::<String>(1);
        let settings = WebDavSyncSettings {
            enabled: true,
            auto_sync: true,
            scope: WebDavSyncScope {
                providers: false,
                mcp: false,
                prompts: true,
            },
            ..WebDavSyncSettings::default()
        };

        take_pending_changed_tables();

        record_changed_table("mcp_servers");
        assert!(enqueue_change_signal(&tx, "mcp_servers"));

        record_changed_table("prompts");
        assert!(!enqueue_change_signal(&tx, "prompts"));

        let changed_tables = take_pending_changed_tables();
        assert!(changed_tables.contains(&String::from("mcp_servers")));
        assert!(changed_tables.contains(&String::from("prompts")));
        assert!(should_auto_sync_for_tables(
            Some(&settings),
            &changed_tables
        ));
    }

    #[test]
    fn suppression_guard_enables_and_restores_state() {
        assert!(!is_auto_sync_suppressed());
        {
            let _guard = AutoSyncSuppressionGuard::new();
            assert!(is_auto_sync_suppressed());
        }
        assert!(!is_auto_sync_suppressed());
    }

    #[test]
    fn max_wait_caps_flush_latency_for_continuous_events() {
        let started = Instant::now();
        let later = started + Duration::from_millis(MAX_AUTO_SYNC_WAIT_MS + 1);
        assert!(auto_sync_wait_duration(started, later).is_none());
    }

    #[tokio::test]
    async fn enqueue_change_signal_drops_when_channel_is_full() {
        let (tx, _rx) = channel::<String>(1);
        assert!(enqueue_change_signal(&tx, "providers"));
        assert!(!enqueue_change_signal(&tx, "providers"));
    }

    #[test]
    fn should_run_auto_sync_requires_enabled_and_auto_sync_flag() {
        assert!(!should_run_auto_sync(None));

        let disabled = WebDavSyncSettings {
            enabled: false,
            auto_sync: true,
            ..WebDavSyncSettings::default()
        };
        assert!(!should_run_auto_sync(Some(&disabled)));

        let auto_sync_off = WebDavSyncSettings {
            enabled: true,
            auto_sync: false,
            ..WebDavSyncSettings::default()
        };
        assert!(!should_run_auto_sync(Some(&auto_sync_off)));

        let enabled = WebDavSyncSettings {
            enabled: true,
            auto_sync: true,
            ..WebDavSyncSettings::default()
        };
        assert!(should_run_auto_sync(Some(&enabled)));
    }

    #[test]
    fn service_layer_does_not_depend_on_commands_layer() {
        let source = include_str!("webdav_auto_sync.rs");
        let needle = ["crate", "commands", ""].join("::");
        assert!(
            !source.contains(&needle),
            "services layer should not depend on commands layer"
        );
    }
}
