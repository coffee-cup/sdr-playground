//! Durable settings storage: a single redb table holding the serialized [`Settings`] blob in the
//! platform app-support directory. There is no manual save action; `SdrApp` writes through here
//! whenever state changes. All operations degrade to a no-op if the store is unavailable, so a
//! read-only or missing directory never blocks the app from running on defaults.

use directories::ProjectDirs;
use redb::{Database, ReadableDatabase, TableDefinition};

use crate::settings::Settings;

const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("settings");
const KEY: &str = "v1";

pub struct Store {
    db: Database,
}

impl Store {
    /// Open (creating if needed) the settings database under the app-support dir. Returns `None`
    /// if the location is unavailable; the app then runs on in-memory defaults.
    pub fn open() -> Option<Store> {
        let dirs = ProjectDirs::from("dev", "sdr-playground", "SDR")?;
        let dir = dirs.data_dir();
        std::fs::create_dir_all(dir).ok()?;
        let db = Database::create(dir.join("settings.redb")).ok()?;
        Some(Store { db })
    }

    /// The persisted settings, or `None` on first run / unreadable data.
    pub fn load(&self) -> Option<Settings> {
        let txn = self.db.begin_read().ok()?;
        let table = txn.open_table(TABLE).ok()?;
        let bytes = table.get(KEY).ok()??;
        serde_json::from_slice(bytes.value()).ok()
    }

    /// Overwrite the persisted settings. Best-effort: failures are swallowed.
    pub fn save(&self, settings: &Settings) {
        let Ok(bytes) = serde_json::to_vec(settings) else {
            return;
        };
        let Ok(txn) = self.db.begin_write() else {
            return;
        };
        {
            let Ok(mut table) = txn.open_table(TABLE) else {
                return;
            };
            let _ = table.insert(KEY, bytes.as_slice());
        }
        let _ = txn.commit();
    }
}
