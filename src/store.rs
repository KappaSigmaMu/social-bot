use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;

#[derive(Debug)]
pub struct OverrideStore {
    conn: Connection,
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
}
