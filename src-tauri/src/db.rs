use crate::model::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Database {
    connection: std::sync::Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
        migrate(&mut connection)?;
        Ok(Self {
            connection: std::sync::Mutex::new(connection),
        })
    }

    pub fn providers(&self) -> anyhow::Result<Vec<Provider>> {
        let connection = self.connection.lock().unwrap();
        let mut query = connection.prepare(
            "SELECT id,name,kind,base_url,default_model,is_default
             FROM providers ORDER BY is_default DESC,name",
        )?;
        let providers = query
            .query_map([], provider_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(providers)
    }

    pub fn provider(&self, id: &str) -> anyhow::Result<Option<Provider>> {
        let connection = self.connection.lock().unwrap();
        Ok(connection
            .query_row(
                "SELECT id,name,kind,base_url,default_model,is_default FROM providers WHERE id=?",
                [id],
                provider_from_row,
            )
            .optional()?)
    }

    pub fn default_provider(&self) -> anyhow::Result<Option<Provider>> {
        let connection = self.connection.lock().unwrap();
        Ok(connection
            .query_row(
                "SELECT id,name,kind,base_url,default_model,is_default
                 FROM providers ORDER BY is_default DESC,name LIMIT 1",
                [],
                provider_from_row,
            )
            .optional()?)
    }

    pub fn save_provider(&self, provider: &Provider) -> anyhow::Result<()> {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        let count: i64 =
            transaction.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))?;
        let is_default = provider.is_default || count == 0;
        if is_default {
            transaction.execute("UPDATE providers SET is_default=0", [])?;
        }
        transaction.execute(
            "INSERT INTO providers(id,name,kind,base_url,default_model,is_default) VALUES(?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, kind=excluded.kind, base_url=excluded.base_url,
               default_model=excluded.default_model, is_default=excluded.is_default",
            params![
                provider.id,
                provider.name,
                provider.kind,
                provider.base_url,
                provider.default_model,
                is_default
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn set_default(&self, id: &str) -> anyhow::Result<()> {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        if !transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?)",
            [id],
            |row| row.get::<_, bool>(0),
        )? {
            anyhow::bail!("Provider not found");
        }
        transaction.execute("UPDATE providers SET is_default=(id=?)", [id])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_provider(&self, id: &str) -> anyhow::Result<()> {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM providers WHERE id=?", [id])?;
        let defaults: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM providers WHERE is_default=1",
            [],
            |row| row.get(0),
        )?;
        if defaults == 0 {
            transaction.execute(
                "UPDATE providers SET is_default=1
                 WHERE id=(SELECT id FROM providers ORDER BY name LIMIT 1)",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn applications(&self) -> anyhow::Result<Vec<Application>> {
        let connection = self.connection.lock().unwrap();
        let mut query = connection.prepare(
            "SELECT id,name,port,api_key,provider_id,enabled,model_access,allowed_models FROM applications ORDER BY name",
        )?;
        let applications = query
            .query_map([], application_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(applications)
    }

    pub fn application(&self, id: &str) -> anyhow::Result<Option<Application>> {
        let connection = self.connection.lock().unwrap();
        Ok(connection
            .query_row(
                "SELECT id,name,port,api_key,provider_id,enabled,model_access,allowed_models FROM applications WHERE id=?",
                [id],
                application_from_row,
            )
            .optional()?)
    }

    pub fn save_application(&self, application: &Application) -> anyhow::Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO applications(id,name,port,api_key,provider_id,enabled,model_access,allowed_models) VALUES(?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, api_key=excluded.api_key,
                provider_id=excluded.provider_id, enabled=excluded.enabled,
                model_access=excluded.model_access, allowed_models=excluded.allowed_models",
            params![
                application.id,
                application.name,
                application.port,
                application.api_key,
                application.provider_id,
                application.enabled,
                serde_json::to_string(&application.model_access)?,
                serde_json::to_string(&application.allowed_models)?
            ],
        )?;
        Ok(())
    }

    pub fn delete_application(&self, id: &str) -> anyhow::Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM applications WHERE id=?", [id])?;
        Ok(())
    }

    pub fn record(&self, record: &RequestRecord<'_>) -> anyhow::Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO requests(
                application_id,provider_id,model,endpoint,modality,status,
                tokens_in,tokens_out,duration_ms,request_bytes,response_bytes,
                error_type,created_at
             ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                record.application_id,
                record.provider_id,
                record.model,
                record.endpoint,
                record.modality,
                record.status,
                record.tokens_in,
                record.tokens_out,
                record.duration_ms,
                record.request_bytes,
                record.response_bytes,
                record.error_type,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn prune_requests(&self, retention_days: u64) -> anyhow::Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        Ok(self.connection.lock().unwrap().execute(
            "DELETE FROM requests WHERE created_at < ?",
            [cutoff.to_rfc3339()],
        )?)
    }

    pub fn stats(&self) -> anyhow::Result<DashboardStats> {
        let connection = self.connection.lock().unwrap();
        let (requests, success, input, output, average): (u64, u64, u64, u64, f64) = connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(status<400),0),
                        COALESCE(SUM(tokens_in),0), COALESCE(SUM(tokens_out),0),
                        COALESCE(AVG(duration_ms),0) FROM requests",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )?;
        let p95_duration_ms = percentile_95(&connection, requests)?;
        let mut query = connection.prepare(
            "SELECT COALESCE(p.name,'Unavailable'),
                    COALESCE(SUM(r.tokens_in),0),COALESCE(SUM(r.tokens_out),0),
                    COALESCE(AVG(CASE WHEN r.duration_ms > 0
                                      THEN r.tokens_out * 1000.0 / r.duration_ms END),0)
             FROM requests r
             LEFT JOIN providers p ON p.id=r.provider_id
             GROUP BY r.provider_id,p.name
             ORDER BY SUM(r.tokens_in + r.tokens_out) DESC,1",
        )?;
        let provider_tokens = query
            .query_map([], |row| {
                Ok(ProviderTokenStat {
                    provider: row.get(0)?,
                    tokens_in: row.get(1)?,
                    tokens_out: row.get(2)?,
                    average_tokens_per_second: row.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        let mut query = connection.prepare(
            "SELECT r.id,COALESCE(a.name,'Deleted'),COALESCE(p.name,'Unavailable'),
                    r.model,r.endpoint,r.modality,r.status,r.tokens_in,r.tokens_out,
                    r.accumulated_tokens_in,r.accumulated_tokens_out,
                    r.duration_ms,r.request_bytes,r.response_bytes,r.error_type,r.created_at
             FROM (
                 SELECT *,
                        SUM(tokens_in) OVER (ORDER BY id ROWS UNBOUNDED PRECEDING) accumulated_tokens_in,
                        SUM(tokens_out) OVER (ORDER BY id ROWS UNBOUNDED PRECEDING) accumulated_tokens_out
                 FROM requests
             ) r
             LEFT JOIN applications a ON a.id=r.application_id
             LEFT JOIN providers p ON p.id=r.provider_id
             ORDER BY r.id DESC LIMIT 100",
        )?;
        let recent = query
            .query_map([], |row| {
                Ok(RequestStat {
                    id: row.get(0)?,
                    application: row.get(1)?,
                    provider: row.get(2)?,
                    model: row.get(3)?,
                    endpoint: row.get(4)?,
                    modality: row.get(5)?,
                    status: row.get(6)?,
                    tokens_in: row.get(7)?,
                    tokens_out: row.get(8)?,
                    accumulated_tokens_in: row.get(9)?,
                    accumulated_tokens_out: row.get(10)?,
                    duration_ms: row.get(11)?,
                    request_bytes: row.get(12)?,
                    response_bytes: row.get(13)?,
                    error_type: row.get(14)?,
                    created_at: row.get(15)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        let mut query = connection.prepare(
            "SELECT substr(created_at,1,10),COUNT(*) FROM requests
             WHERE created_at >= ? GROUP BY 1 ORDER BY 1",
        )?;
        let today = chrono::Utc::now().date_naive();
        let first_day = today - chrono::Duration::days(29);
        let cutoff = format!("{first_day}T00:00:00Z");
        let daily_counts: std::collections::HashMap<String, u64> = query
            .query_map([cutoff], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let daily = (0..30)
            .map(|offset| {
                let day = (first_day + chrono::Duration::days(offset)).to_string();
                DailyStat {
                    requests: daily_counts.get(&day).copied().unwrap_or_default(),
                    day,
                }
            })
            .collect();
        Ok(DashboardStats {
            requests,
            success_rate: if requests == 0 {
                0.0
            } else {
                success as f64 * 100.0 / requests as f64
            },
            tokens_in: input,
            tokens_out: output,
            average_duration_ms: average,
            p95_duration_ms,
            provider_tokens,
            daily,
            recent,
        })
    }

    pub fn provider_usage(&self) -> anyhow::Result<Vec<ProviderUsage>> {
        crate::provider_stats::list_usage(
            &self.connection.lock().unwrap(),
            chrono::Utc::now().date_naive(),
        )
    }

    pub fn provider_stats(&self, id: &str) -> anyhow::Result<ProviderStats> {
        crate::provider_stats::stats(
            &self.connection.lock().unwrap(),
            id,
            chrono::Utc::now().date_naive(),
        )
    }

    pub fn set_billing_start_day(&self, id: &str, day: u32) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=31).contains(&day),
            "Billing start day must be between 1 and 31"
        );
        let connection = self.connection.lock().unwrap();
        anyhow::ensure!(
            connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM providers WHERE id=?)",
                [id],
                |row| row.get::<_, bool>(0)
            )?,
            "Provider not found"
        );
        connection.execute("INSERT INTO provider_billing(provider_id,start_day) VALUES(?,?) ON CONFLICT(provider_id) DO UPDATE SET start_day=excluded.start_day", params![id, day])?;
        Ok(())
    }
}

fn provider_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Provider> {
    Ok(Provider {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        base_url: row.get(3)?,
        default_model: row.get(4)?,
        is_default: row.get(5)?,
        has_secret: false,
    })
}

fn application_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Application> {
    Ok(Application {
        id: row.get(0)?,
        name: row.get(1)?,
        port: row.get(2)?,
        api_key: row.get(3)?,
        provider_id: row.get(4)?,
        enabled: row.get(5)?,
        model_access: read_json_column(row, 6)?,
        allowed_models: read_json_column(row, 7)?,
    })
}

fn read_json_column<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let value: String = row.get(index)?;
    serde_json::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn percentile_95(connection: &Connection, count: u64) -> anyhow::Result<f64> {
    if count == 0 {
        return Ok(0.0);
    }
    let offset = ((count - 1) as f64 * 0.95).round() as u64;
    Ok(connection.query_row(
        "SELECT duration_ms FROM requests ORDER BY duration_ms LIMIT 1 OFFSET ?",
        [offset],
        |row| row.get::<_, f64>(0),
    )?)
}

fn migrate(connection: &mut Connection) -> anyhow::Result<()> {
    let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS providers(
               id TEXT PRIMARY KEY,name TEXT NOT NULL,kind TEXT NOT NULL,
               base_url TEXT NOT NULL,default_model TEXT NOT NULL,
               is_default INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS applications(
               id TEXT PRIMARY KEY,name TEXT NOT NULL,port INTEGER UNIQUE NOT NULL,
               api_key TEXT NOT NULL,provider_id TEXT REFERENCES providers(id) ON DELETE SET NULL,
               enabled INTEGER NOT NULL DEFAULT 1);
             CREATE TABLE IF NOT EXISTS requests(
               id INTEGER PRIMARY KEY AUTOINCREMENT,application_id TEXT,provider_id TEXT,
               model TEXT,status INTEGER,tokens_in INTEGER,tokens_out INTEGER,
               duration_ms INTEGER,created_at TEXT NOT NULL,request_body TEXT,response_body TEXT);
             CREATE INDEX IF NOT EXISTS requests_created_at ON requests(created_at);
             PRAGMA user_version=1;",
        )?;
    }
    if version < 2 {
        connection.execute_batch(
            "ALTER TABLE requests ADD COLUMN endpoint TEXT NOT NULL DEFAULT '';
             ALTER TABLE requests ADD COLUMN modality TEXT NOT NULL DEFAULT 'unknown';
             ALTER TABLE requests ADD COLUMN request_bytes INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE requests ADD COLUMN response_bytes INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE requests ADD COLUMN error_type TEXT;
             UPDATE requests SET request_body=NULL,response_body=NULL;
             PRAGMA user_version=2;",
        )?;
    }
    if version < 3 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TABLE provider_billing(provider_id TEXT PRIMARY KEY REFERENCES providers(id) ON DELETE CASCADE,
                start_day INTEGER NOT NULL DEFAULT 1 CHECK(start_day BETWEEN 1 AND 31));
             CREATE INDEX requests_provider_date ON requests(provider_id,created_at);
             PRAGMA user_version=3;",
        )?;
        transaction.commit()?;
    }
    if version < 4 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "ALTER TABLE applications ADD COLUMN model_access TEXT NOT NULL DEFAULT '\"defaultOnly\"';
             ALTER TABLE applications ADD COLUMN allowed_models TEXT NOT NULL DEFAULT '[]';
             PRAGMA user_version=4;",
        )?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_persists_application_model_access() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gateway.db");
        let db = Database::open(&path).unwrap();
        let mut application = Application {
            id: "app".into(),
            name: "App".into(),
            port: 43100,
            api_key: "secret".into(),
            provider_id: None,
            enabled: true,
            model_access: ModelAccess::Selected,
            allowed_models: vec!["one".into(), "two".into()],
        };
        db.save_application(&application).unwrap();
        assert_eq!(
            db.applications().unwrap()[0].allowed_models,
            application.allowed_models
        );
        application.model_access = ModelAccess::All;
        db.save_application(&application).unwrap();
        assert_eq!(
            db.application("app").unwrap().unwrap().model_access,
            ModelAccess::All
        );
        {
            let connection = db.connection.lock().unwrap();
            connection
                .execute_batch(
                    "ALTER TABLE applications DROP COLUMN model_access;
                 ALTER TABLE applications DROP COLUMN allowed_models;
                 PRAGMA user_version=3;",
                )
                .unwrap();
        }
        drop(db);
        let db = Database::open(&path).unwrap();
        let row = db.application("app").unwrap().unwrap();
        assert_eq!(row.model_access, ModelAccess::DefaultOnly);
        assert!(row.allowed_models.is_empty());
        assert_eq!(row.port, application.port);
        assert_eq!(row.api_key, application.api_key);
        db.connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE applications SET model_access=? WHERE id=?",
                params!["invalid", "app"],
            )
            .unwrap();
        assert!(db.application("app").is_err());
    }

    fn provider(id: &str) -> Provider {
        Provider {
            id: id.into(),
            name: id.into(),
            kind: "openai".into(),
            base_url: "http://example.test/v1".into(),
            default_model: "model".into(),
            is_default: false,
            has_secret: false,
        }
    }

    #[test]
    fn provider_application_and_statistics_lifecycle() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(&directory.path().join("agenthub.db")).unwrap();
        database.save_provider(&provider("p1")).unwrap();
        database.save_provider(&provider("p2")).unwrap();
        assert_eq!(database.providers().unwrap().len(), 2);
        assert!(database.provider("p1").unwrap().unwrap().is_default);
        database.set_default("p2").unwrap();
        assert!(database.set_default("missing").is_err());

        let mut application = Application {
            id: "a".into(),
            name: "Editor".into(),
            port: 43100,
            api_key: "old".into(),
            provider_id: Some("p2".into()),
            enabled: true,
            model_access: ModelAccess::DefaultOnly,
            allowed_models: vec![],
        };
        database.save_application(&application).unwrap();
        assert_eq!(database.applications().unwrap().len(), 1);
        application.api_key = "new".into();
        database.save_application(&application).unwrap();
        assert_eq!(database.application("a").unwrap().unwrap().api_key, "new");

        database
            .record(&RequestRecord {
                application_id: "a",
                provider_id: Some("p2"),
                model: "model",
                endpoint: "/v1/responses",
                modality: "text",
                status: 200,
                tokens_in: 2,
                tokens_out: 3,
                duration_ms: 5,
                request_bytes: 10,
                response_bytes: 20,
                error_type: None,
            })
            .unwrap();
        let stats = database.stats().unwrap();
        assert_eq!(stats.tokens_out, 3);
        assert_eq!(stats.provider_tokens.len(), 1);
        assert_eq!(stats.provider_tokens[0].provider, "p2");
        assert_eq!(stats.provider_tokens[0].tokens_in, 2);
        assert_eq!(stats.provider_tokens[0].tokens_out, 3);
        assert_eq!(stats.provider_tokens[0].average_tokens_per_second, 600.0);
        assert_eq!(stats.p95_duration_ms, 5.0);
        assert_eq!(stats.recent[0].accumulated_tokens_in, 2);
        assert_eq!(stats.recent[0].accumulated_tokens_out, 3);
        assert_eq!(stats.daily.len(), 30);
        assert_eq!(stats.daily.last().unwrap().requests, 1);
        assert_eq!(database.prune_requests(0).unwrap(), 0);
        assert_eq!(database.prune_requests(1).unwrap(), 0);

        database.delete_provider("p2").unwrap();
        assert!(database.default_provider().unwrap().unwrap().is_default);
        database.delete_application("a").unwrap();

        drop(database);
        let reopened = Database::open(&directory.path().join("agenthub.db")).unwrap();
        let empty_directory = tempfile::tempdir().unwrap();
        let empty = Database::open(&empty_directory.path().join("empty.db")).unwrap();
        let stats = empty.stats().unwrap();
        assert_eq!(stats.requests, 0);
        assert_eq!(stats.p95_duration_ms, 0.0);
        assert!(stats.provider_tokens.is_empty());
        assert!(stats.daily.iter().all(|day| day.requests == 0));
        assert!(reopened.applications().unwrap().is_empty());
    }

    #[test]
    fn provider_billing_statistics_and_migration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stats.db");
        let database = Database::open(&path).unwrap();
        let mut first = provider("p1");
        first.name = "Same name".into();
        let mut second = provider("p2");
        second.name = first.name.clone();
        database.save_provider(&first).unwrap();
        database.save_provider(&second).unwrap();
        assert!(database.set_billing_start_day("p1", 0).is_err());
        assert!(database.set_billing_start_day("p1", 32).is_err());
        assert!(database.set_billing_start_day("missing", 15).is_err());
        assert!(database.provider_stats("missing").is_err());
        assert_eq!(database.provider_stats("p1").unwrap().billing_start_day, 1);
        assert_eq!(database.provider_usage().unwrap().len(), 2);
        {
            let connection = database.connection.lock().unwrap();
            // Recreate a v2 database to verify the upgrade preserves existing data.
            connection
                .execute_batch(
                    "DROP TABLE provider_billing; DROP INDEX requests_provider_date;
                ALTER TABLE applications DROP COLUMN model_access;
                ALTER TABLE applications DROP COLUMN allowed_models;
                PRAGMA user_version=2;",
                )
                .unwrap();
            for (id, timestamp, input, output, duration) in [
                ("p1", "2025-12-01T00:00:00Z", 10, 20, 1000),
                ("p1", "2026-10-14T23:59:59Z", 1, 1, 0),
                ("p1", "2026-10-15T00:00:00Z", 2, 4, 1000),
                ("p1", "2026-11-11T12:00:00Z", 3, 6, 2000),
                ("p1", "2026-11-11T13:00:00Z", 5, 10, 0),
                ("p1", "2026-11-15T00:00:00Z", 7, 14, 1000),
                ("p2", "2026-11-11T12:00:00Z", 100, 200, 1000),
            ] {
                connection.execute("INSERT INTO requests(provider_id,created_at,tokens_in,tokens_out,duration_ms) VALUES(?,?,?,?,?)", params![id, timestamp, input, output, duration]).unwrap();
            }
        }
        drop(database);
        let database = Database::open(&path).unwrap();
        assert_eq!(database.provider_stats("p1").unwrap().billing_start_day, 1);
        database.set_billing_start_day("p1", 15).unwrap();
        database.set_billing_start_day("p1", 16).unwrap();
        database.set_billing_start_day("p1", 15).unwrap();
        first.name = "Renamed".into();
        database.save_provider(&first).unwrap();
        let today = "2026-11-11".parse().unwrap();
        {
            let connection = database.connection.lock().unwrap();
            let stats = crate::provider_stats::stats(&connection, "p1", today).unwrap();
            assert_eq!(stats.provider_name, "Renamed");
            assert_eq!(stats.billing_start_day, 15);
            assert_eq!(stats.period_start, "2026-10-15");
            assert_eq!(stats.period_end, "2026-11-14");
            assert_eq!(stats.current.requests, 3);
            assert_eq!(stats.current.tokens_in, 10);
            assert_eq!(stats.current.tokens_out, 20);
            assert_eq!(stats.current.total_tokens, 30);
            assert_eq!(stats.current.average_tokens, 10.0);
            assert_eq!(stats.current.average_tokens_per_second, 3.5);
            assert_eq!(stats.historical.total_tokens, 83);
            assert_eq!(stats.daily.len(), 30);
            assert_eq!(stats.daily[0].date, "2026-10-13");
            assert_eq!(stats.daily[0].usage.total_tokens, 0);
            assert_eq!(stats.daily.last().unwrap().usage.total_tokens, 24);
            assert_eq!(
                stats.daily.last().unwrap().usage.average_tokens_per_second,
                3.0
            );
            assert_eq!(stats.monthly.len(), 12);
            assert_eq!(stats.monthly[0].date, "2025-12");
            assert_eq!(stats.monthly[0].usage.total_tokens, 30);
            assert_eq!(stats.monthly[1].usage.total_tokens, 0);
            assert_eq!(stats.monthly.last().unwrap().usage.total_tokens, 24);
            let usage = crate::provider_stats::list_usage(&connection, today).unwrap();
            let first = usage.iter().find(|item| item.provider_id == "p1").unwrap();
            assert_eq!(first.total_tokens, 83);
            assert_eq!(first.month_tokens, 30);
            assert_eq!(first.period_start, "2026-10-15");
            assert_eq!(first.period_end, "2026-11-14");
            assert_eq!(
                usage
                    .iter()
                    .find(|item| item.provider_id == "p2")
                    .unwrap()
                    .month_tokens,
                300
            );
            let json = serde_json::to_value(stats).unwrap();
            assert_eq!(json["daily"][29]["averageTokensPerSecond"], 3.0);
            assert_eq!(json["current"]["totalTokens"], 30);
        }
        drop(database);
        let database = Database::open(&path).unwrap();
        assert_eq!(database.provider_stats("p1").unwrap().billing_start_day, 15);
        database.delete_provider("p1").unwrap();
        assert_eq!(
            database
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM provider_billing", [], |row| row
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        database.save_provider(&provider("empty")).unwrap();
        let stats = database.provider_stats("empty").unwrap();
        assert_eq!(stats.historical.requests, 0);
        assert_eq!(stats.historical.average_tokens, 0.0);
        assert_eq!(stats.historical.average_tokens_per_second, 0.0);
        assert!(stats
            .daily
            .iter()
            .all(|bucket| bucket.usage.total_tokens == 0));
        assert!(stats
            .monthly
            .iter()
            .all(|bucket| bucket.usage.total_tokens == 0));
    }
}
