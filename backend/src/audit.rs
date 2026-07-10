//! Operational audit trail (Impl Spec §5.2).
//!
//! Failed authentications and authorizations are appended here as JSON lines.
//! This log is intentionally separate from the accounting ledger: it records
//! operational security events, not accounting events.

use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct OpsAudit {
    path: PathBuf,
    lock: Mutex<()>,
}

#[derive(Serialize)]
struct AuditRecord<'a> {
    ts_unix: u64,
    event: &'a str,
    subject: &'a str,
    outcome: &'a str,
    detail: &'a str,
}

impl OpsAudit {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn record(&self, event: &str, subject: &str, outcome: &str, detail: &str) {
        let ts_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let record = AuditRecord {
            ts_unix,
            event,
            subject,
            outcome,
            detail,
        };
        if let Ok(line) = serde_json::to_string(&record) {
            let _guard = self.lock.lock().expect("audit lock poisoned");
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&self.path) {
                let _ = writeln!(file, "{line}");
            }
        }
    }
}
