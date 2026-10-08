//! Revision-bound daily views. Source changes remove results and queue requested views again.
use crate::{
    authentication::{digest, now},
    database::{Database, queries},
    error::AppError,
};
use serde_json::Value;
use sqlx::Row;
impl Database {
    pub async fn wellness_day(
        &self,
        user: &str,
        request: super::DayRequest,
    ) -> Result<Value, AppError> {
        let parameters = serde_json::to_string(&request)?;
        let key = digest(parameters.as_bytes());
        let cached: Option<String> = sqlx::query_scalar(queries::DAILY_VIEW_GET)
            .bind(user)
            .bind(&key)
            .bind(user)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(cached) = cached {
            return Ok(serde_json::from_str(&cached)?);
        }
        let result = self.compute_wellness_day(user, request).await?;
        let revision = result["data_revision"].as_i64().ok_or(AppError::Internal)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let current: Option<i64> = sqlx::query_scalar(queries::USER_DATA_REVISION)
            .bind(user)
            .fetch_optional(&mut *tx)
            .await?;
        if current != Some(revision) {
            return Err(AppError::Conflict(
                "Archive changed; refresh the daily view",
            ));
        }
        sqlx::query(queries::DAILY_VIEW_SAVE)
            .bind(user)
            .bind(&key)
            .bind(parameters)
            .bind(revision)
            .bind(serde_json::to_string(&result)?)
            .bind(now()?)
            .execute(&mut *tx)
            .await?;
        sqlx::query(queries::DAILY_VIEW_LIMIT)
            .bind(user)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn recompute_daily_next(
        &self,
        at: i64,
        lease_seconds: i64,
    ) -> Result<bool, AppError> {
        if !(30..=3600).contains(&lease_seconds) {
            return Err(AppError::Invalid("Invalid daily-view lease"));
        }
        sqlx::query(queries::DAILY_VIEW_EXHAUSTED)
            .bind(at)
            .execute(&self.pool)
            .await?;
        let token = uuid::Uuid::new_v4().to_string();
        let lease = at
            .checked_add(lease_seconds)
            .ok_or(AppError::Invalid("Lease overflow"))?;
        let Some(row) = sqlx::query(queries::DAILY_VIEW_CLAIM)
            .bind(&token)
            .bind(lease)
            .bind(at)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(false);
        };
        let user: String = row.get("user_id");
        let key: String = row.get("request_key");
        let revision: i64 = row.get("input_revision");
        let computation = async {
            let request: super::DayRequest =
                serde_json::from_str(&row.get::<String, _>("parameters_json"))?;
            self.compute_wellness_day(&user, request).await
        };
        // A bounded attempt completes before its lease. Expired claims recover after process interruption.
        let result = tokio::time::timeout(
            std::time::Duration::from_secs((lease_seconds - 5) as u64),
            computation,
        )
        .await;
        match result {
            Ok(Ok(value)) => {
                sqlx::query(queries::DAILY_VIEW_FINISH)
                    .bind(serde_json::to_string(&value)?)
                    .bind(&user)
                    .bind(&key)
                    .bind(&token)
                    .bind(revision)
                    .bind(&user)
                    .execute(&self.pool)
                    .await?;
            }
            _ => {
                sqlx::query(queries::DAILY_VIEW_FAIL)
                    .bind(&user)
                    .bind(&key)
                    .bind(&token)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(true)
    }
}
