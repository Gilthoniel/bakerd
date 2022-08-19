use super::{AsyncPool, StorageError};
use crate::model::Status;
use crate::schema::statuses::dsl::*;
use diesel::prelude::*;

pub use records::{NewStatus, ResourceStatusJson};

pub mod records {
    use crate::schema::statuses;
    use diesel::backend;
    use diesel::deserialize as de;
    use diesel::serialize as se;
    use diesel::sql_types::Text;
    use diesel::sqlite::Sqlite;
    use serde::{Deserialize, Serialize};

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

    #[derive(Queryable)]
    pub struct Status {
        pub id: i32,
        pub resources: ResourceStatusJson,
        pub timestamp_ms: i64,
    }

    #[derive(Insertable)]
    #[diesel(table_name = statuses)]
    pub struct NewStatus {
        pub timestamp_ms: i64,
        pub resources: ResourceStatusJson,
    }
}

#[async_trait]
pub trait StatusRepository {
    async fn get_last_report(&self) -> Result<Status, StorageError>;

    async fn report(&self, status: NewStatus) -> Result<(), StorageError>;
}

pub struct SqliteStatusRepository {
    pool: AsyncPool,
}

impl SqliteStatusRepository {
    pub fn new(pool: AsyncPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StatusRepository for SqliteStatusRepository {
    async fn get_last_report(&self) -> Result<Status, StorageError> {
        let res: records::Status = self
            .pool
            .exec(|mut conn| statuses.order_by(timestamp_ms.desc()).first(&mut conn))
            .await?;

        Ok(Status::from(res))
    }

    async fn report(&self, status: NewStatus) -> Result<(), StorageError> {
        self.pool
            .exec(move |mut conn| {
                diesel::insert_into(statuses)
                    .values(&status)
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_report_and_get() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        let repository = SqliteStatusRepository::new(pool);

        let new_status = NewStatus {
            resources: ResourceStatusJson {
                avg_cpu_load: None,
                mem_free: None,
                mem_total: None,
                uptime_secs: None,
            },
            timestamp_ms: 1000,
        };

        assert!(matches!(repository.report(new_status).await, Ok(_)));

        let res = repository.get_last_report().await;

        assert!(matches!(res, Ok(_)));
    }
}
