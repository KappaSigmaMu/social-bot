use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;

#[derive(Debug)]
pub struct OverrideStore {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundRoot {
    pub root_event_id: String,
    pub started_relay_block: u64,
}

impl OverrideStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("opening override database")?;
        let store = Self { conn };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS accounts (
                address TEXT PRIMARY KEY,
                matrix_handle TEXT NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS seen_society_events (
                block_hash TEXT NOT NULL,
                event_index INTEGER NOT NULL,
                kind TEXT NOT NULL,
                PRIMARY KEY (block_hash, event_index, kind)
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS round_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                root_event_id TEXT NOT NULL,
                started_relay_block INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn is_society_event_seen(
        &self,
        block_hash: &str,
        event_index: u32,
        kind: &str,
    ) -> Result<bool> {
        let mut statement = self
            .conn
            .prepare("SELECT 1 FROM seen_society_events WHERE block_hash = ?1 AND event_index = ?2 AND kind = ?3")?;
        let result = statement.query_row(params![block_hash, event_index, kind], |_| Ok(()));
        match result {
            Ok(()) => Ok(true),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    /// Returns `true` when the event was newly recorded, `false` if it was already seen.
    pub fn mark_society_event_seen(
        &self,
        block_hash: &str,
        event_index: u32,
        kind: &str,
    ) -> Result<bool> {
        let rows = self.conn.execute(
            "INSERT OR IGNORE INTO seen_society_events (block_hash, event_index, kind)
             VALUES (?1, ?2, ?3)",
            params![block_hash, event_index, kind],
        )?;
        Ok(rows > 0)
    }

    pub fn round_root(&self) -> Result<Option<RoundRoot>> {
        let mut statement = self
            .conn
            .prepare("SELECT root_event_id, started_relay_block FROM round_state WHERE id = 1")?;
        let result = statement.query_row([], |row| {
            Ok(RoundRoot {
                root_event_id: row.get(0)?,
                started_relay_block: row.get(1)?,
            })
        });
        match result {
            Ok(root) => Ok(Some(root)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn set_round_root(&self, root_event_id: &str, relay_block: u64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO round_state (id, root_event_id, started_relay_block)
             VALUES (1, ?1, ?2)",
            params![root_event_id, relay_block],
        )?;
        Ok(())
    }

    pub fn clear_round_root(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM round_state WHERE id = 1", [])?;
        Ok(())
    }

    pub fn set_matrix_handle(&self, address: &str, matrix_handle: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO accounts (address, matrix_handle) VALUES (?1, ?2)",
            params![address, matrix_handle],
        )?;
        Ok(())
    }

    pub fn unset_by_matrix_handle(&self, matrix_handle: &str) -> Result<bool> {
        let rows = self.conn.execute(
            "DELETE FROM accounts WHERE matrix_handle = ?1",
            params![matrix_handle],
        )?;
        Ok(rows > 0)
    }

    pub fn matrix_handle_for_address(&self, address: &str) -> Result<Option<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT matrix_handle FROM accounts WHERE address = ?1")?;
        let result = statement.query_row(params![address], |row| row.get(0));
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn address_for_matrix_handle(&self, matrix_handle: &str) -> Result<Option<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT address FROM accounts WHERE matrix_handle = ?1")?;
        let result = statement.query_row(params![matrix_handle], |row| row.get(0));
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn list_overrides(&self) -> Result<Vec<(String, String)>> {
        let mut statement = self
            .conn
            .prepare("SELECT address, matrix_handle FROM accounts ORDER BY matrix_handle")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn store() -> OverrideStore {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        OverrideStore::open(path).unwrap()
    }

    #[test]
    fn creates_updates_reads_and_deletes_overrides() {
        let store = store();
        assert_eq!(store.matrix_handle_for_address("addr").unwrap(), None);
        assert_eq!(
            store.address_for_matrix_handle("@user:matrix.org").unwrap(),
            None
        );

        store.set_matrix_handle("addr", "@user:matrix.org").unwrap();
        assert_eq!(
            store.matrix_handle_for_address("addr").unwrap().as_deref(),
            Some("@user:matrix.org")
        );
        assert_eq!(
            store
                .address_for_matrix_handle("@user:matrix.org")
                .unwrap()
                .as_deref(),
            Some("addr")
        );

        store
            .set_matrix_handle("addr", "@updated:matrix.org")
            .unwrap();
        assert_eq!(
            store.matrix_handle_for_address("addr").unwrap().as_deref(),
            Some("@updated:matrix.org")
        );

        assert!(!store.unset_by_matrix_handle("@missing:matrix.org").unwrap());
        assert!(store.unset_by_matrix_handle("@updated:matrix.org").unwrap());
        assert_eq!(store.matrix_handle_for_address("addr").unwrap(), None);
    }

    #[test]
    fn tracks_seen_society_events_across_reopens() {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        let store = OverrideStore::open(&path).unwrap();

        assert!(!store.is_society_event_seen("hash1", 0, "Bid").unwrap());
        assert!(store.mark_society_event_seen("hash1", 0, "Bid").unwrap());
        assert!(!store.mark_society_event_seen("hash1", 0, "Bid").unwrap());
        assert!(store.is_society_event_seen("hash1", 0, "Bid").unwrap());
        assert!(!store.is_society_event_seen("hash1", 1, "Bid").unwrap());
        assert!(store.mark_society_event_seen("hash1", 1, "Vote").unwrap());

        let reopened = OverrideStore::open(&path).unwrap();
        assert!(reopened.is_society_event_seen("hash1", 0, "Bid").unwrap());
        assert!(
            !reopened
                .mark_society_event_seen("hash1", 1, "Vote")
                .unwrap()
        );
    }

    #[test]
    fn round_root_is_a_single_upserted_row() {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        let store = OverrideStore::open(&path).unwrap();

        assert_eq!(store.round_root().unwrap(), None);

        store.set_round_root("$root-1", 100).unwrap();
        assert_eq!(
            store.round_root().unwrap(),
            Some(RoundRoot {
                root_event_id: "$root-1".to_owned(),
                started_relay_block: 100,
            })
        );

        store.set_round_root("$root-2", 200).unwrap();
        assert_eq!(
            store.round_root().unwrap(),
            Some(RoundRoot {
                root_event_id: "$root-2".to_owned(),
                started_relay_block: 200,
            })
        );

        store.clear_round_root().unwrap();
        assert_eq!(store.round_root().unwrap(), None);
    }
}
