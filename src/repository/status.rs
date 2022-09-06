use super::{AsyncPool, Result};
use crate::model::Status;
use crate::schema::statuses::dsl::*;
use diesel::prelude::*;
use std::sync::Arc;

pub mod models {
  use crate::schema::statuses;
  use diesel::backend;
  use diesel::deserialize as de;
  use diesel::serialize as se;
  use diesel::sql_types::{Nullable, Text};
  use diesel::sqlite::Sqlite;
  use serde::{Deserialize, Serialize};

  /// A JSON blob of the resources of the server on which the node is running.
  #[derive(Serialize, Deserialize, AsExpression, FromSqlRow, Debug)]
  #[diesel(sql_type = Text)]
  pub struct ResourceStatusJson {
    pub avg_cpu_load: Option<f32>,
    pub mem_free: Option<u64>,
    pub mem_total: Option<u64>,
    pub uptime_secs: Option<u64>,
  }

  impl se::ToSql<Text, Sqlite> for ResourceStatusJson {
    fn to_sql(&self, out: &mut se::Output<Sqlite>) -> se::Result {
      let value = serde_json::to_string(&self)?;
      out.set_value(value);
      Ok(se::IsNull::No)
    }
  }

  impl de::FromSql<Text, Sqlite> for ResourceStatusJson {
    fn from_sql(value: backend::RawValue<Sqlite>) -> de::Result<Self> {
      let decoded = <String as de::FromSql<Text, Sqlite>>::from_sql(value)?;
      Ok(serde_json::from_str(&decoded)?)
    }
  }

  /// A JSON blob of the node status.
  #[derive(Serialize, Deserialize, AsExpression, FromSqlRow, Debug)]
  #[diesel(sql_type = Nullable<Text>)]
  pub struct NodeStatusJson {
    pub node_id: Option<String>,
    pub baker_id: Option<u64>,
    pub is_baker_committee: bool,
    pub is_finalizer_committee: bool,
    pub uptime_ms: u64,
    pub peer_type: String,
    pub peer_average_latency: f64,
    pub peer_count: usize,
  }

  impl se::ToSql<Nullable<Text>, Sqlite> for NodeStatusJson {
    fn to_sql(&self, out: &mut se::Output<Sqlite>) -> se::Result {
      let value = serde_json::to_string(&self)?;
      out.set_value(value);
      Ok(se::IsNull::No)
    }
  }

  impl de::FromSql<Text, Sqlite> for NodeStatusJson {
    fn from_sql(value: backend::RawValue<Sqlite>) -> de::Result<Self> {
      let decoded = <String as de::FromSql<Text, Sqlite>>::from_sql(value)?;
      Ok(serde_json::from_str(&decoded)?)
    }
  }

  #[derive(Queryable)]
  pub struct Status {
    pub id: i32,
    pub resources: ResourceStatusJson,
    pub node: Option<NodeStatusJson>,
    pub timestamp_ms: i64,
  }

  #[derive(Insertable)]
  #[diesel(table_name = statuses)]
  pub struct NewStatus {
    pub timestamp_ms: i64,
    pub resources: ResourceStatusJson,
    pub node: Option<NodeStatusJson>,
  }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait StatusRepository {
  async fn get_last_report(&self) -> Result<Status>;

  async fn report(&self, status: models::NewStatus) -> Result<()>;

  async fn garbage_collect(&self, after_nth: i64) -> Result<()>;
}

pub type DynStatusRepository = Arc<dyn StatusRepository + Sync + Send>;

pub struct SqliteStatusRepository {
  pool: AsyncPool,
}

impl SqliteStatusRepository {
  pub fn new(pool: AsyncPool) -> Self {
    Self {
      pool,
    }
  }
}

#[async_trait]
impl StatusRepository for SqliteStatusRepository {
  async fn get_last_report(&self) -> Result<Status> {
    let res: models::Status = self
      .pool
      .exec(|mut conn| statuses.order_by(timestamp_ms.desc()).first(&mut conn))
      .await?;

    Ok(Status::from(res))
  }

  async fn report(&self, status: models::NewStatus) -> Result<()> {
    self
      .pool
      .exec(move |mut conn| diesel::insert_into(statuses).values(&status).execute(&mut conn))
      .await?;

    Ok(())
  }

  /// It keeps the most recent nth reports and deletes the other ones.
  async fn garbage_collect(&self, after_nth: i64) -> Result<()> {
    self
      .pool
      .exec(move |mut conn| {
        diesel::delete(statuses)
          .filter(
            id.ne_all(
              statuses
                .select(id)
                .order_by(timestamp_ms.desc())
                .limit(after_nth)
                .into_boxed(),
            ),
          )
          .execute(&mut conn)
      })
      .await?;

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test(flavor = "multi_thread")]
  async fn test_report_and_get() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteStatusRepository::new(pool);

    let new_status = models::NewStatus {
      resources: models::ResourceStatusJson {
        avg_cpu_load: None,
        mem_free: None,
        mem_total: None,
        uptime_secs: None,
      },
      node: Some(models::NodeStatusJson {
        node_id: Some("test".to_string()),
        baker_id: Some(8343),
        is_baker_committee: true,
        is_finalizer_committee: true,
        uptime_ms: 0,
        peer_type: "peer".to_string(),
        peer_average_latency: 0.0,
        peer_count: 5,
      }),
      timestamp_ms: 1000,
    };

    assert!(matches!(repository.report(new_status).await, Ok(_)));

    let res = repository.get_last_report().await;

    assert!(matches!(res, Ok(_)));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_garbage_collect() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteStatusRepository::new(pool.clone());

    // Create two reports.
    let first_report = models::NewStatus {
      resources: models::ResourceStatusJson {
        avg_cpu_load: None,
        mem_free: None,
        mem_total: None,
        uptime_secs: None,
      },
      node: None,
      timestamp_ms: 1000,
    };

    assert!(matches!(repository.report(first_report).await, Ok(_)));

    let second_report = models::NewStatus {
      resources: models::ResourceStatusJson {
        avg_cpu_load: None,
        mem_free: None,
        mem_total: None,
        uptime_secs: None,
      },
      node: None,
      timestamp_ms: 2000,
    };

    assert!(matches!(repository.report(second_report).await, Ok(_)));

    let res = repository.garbage_collect(1).await;

    assert!(matches!(res, Ok(_)));

    let count = pool.exec(|mut conn| statuses.count().execute(&mut conn)).await;

    assert!(matches!(count, Ok(1)));
  }
}
