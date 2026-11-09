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
            "INSERT INTO providers VALUES(?,?,?,?,?,?)
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
            "SELECT id,name,port,api_key,provider_id,enabled FROM applications ORDER BY name",
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
                "SELECT id,name,port,api_key,provider_id,enabled FROM applications WHERE id=?",
                [id],
                application_from_row,
            )
            .optional()?)
    }

    pub fn save_application(&self, application: &Application) -> anyhow::Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO applications VALUES(?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, api_key=excluded.api_key,
               provider_id=excluded.provider_id, enabled=excluded.enabled",
            params![
                application.id,
                application.name,
                application.port,
                application.api_key,
                application.provider_id,
                application.enabled
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
