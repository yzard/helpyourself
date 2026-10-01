use crate::{
    database::{Database, queries},
    error::AppError,
    models::Job,
};
use serde::Serialize;

#[derive(Serialize, sqlx::FromRow)]
pub struct ClaimedJob {
    pub job_id: String,
    pub user_id: String,
    pub file_id: String,
    pub lease_token: String,
    pub lease_until: i64,
}

impl Database {
    /// Called only by a capable worker; the HTTP API cannot activate unavailable extraction.
    pub async fn queue_blocked_job(&self, user_id: &str, job_id: &str) -> Result<(), AppError> {
        self.job(user_id, job_id).await?;
        let updated = sqlx::query(queries::QUEUE_BLOCKED_JOB)
            .bind(user_id)
            .bind(job_id)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict("Job is not blocked"));
        }
        Ok(())
    }

    pub async fn retry_job(&self, user_id: &str, job_id: &str) -> Result<Job, AppError> {
        self.job(user_id, job_id).await?;
        let updated = sqlx::query(queries::RETRY_JOB)
            .bind(user_id)
            .bind(job_id)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict(
                "Only failed jobs can be retried; unavailable extraction cannot run",
            ));
        }
        self.job(user_id, job_id).await
    }

    pub async fn claim_job(
        &self,
        now: i64,
        lease_seconds: i64,
        maximum_attempts: i64,
    ) -> Result<Option<ClaimedJob>, AppError> {
        if lease_seconds <= 0 || maximum_attempts <= 0 {
            return Err(AppError::Invalid("Invalid job lease or attempt limit"));
        }
        let lease_until = now
            .checked_add(lease_seconds)
            .ok_or(AppError::Invalid("Invalid lease deadline"))?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(queries::EXHAUST_JOBS)
            .bind(now)
            .bind(maximum_attempts)
            .execute(&mut *transaction)
            .await?;
        let job = sqlx::query_as(queries::CLAIM_JOB)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(lease_until)
            .bind(now)
            .bind(maximum_attempts)
            .fetch_optional(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(job)
    }

    pub async fn renew_job(
        &self,
        job: &ClaimedJob,
        now: i64,
        lease_seconds: i64,
    ) -> Result<(), AppError> {
        if lease_seconds <= 0 {
            return Err(AppError::Invalid("Invalid job lease"));
        }
        let deadline = now
            .checked_add(lease_seconds)
            .ok_or(AppError::Invalid("Invalid lease deadline"))?;
        let updated = sqlx::query(queries::RENEW_JOB)
            .bind(deadline)
            .bind(&job.job_id)
            .bind(&job.lease_token)
            .bind(now)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict("Job lease is no longer owned"));
        }
        Ok(())
    }

    pub async fn finish_job(
        &self,
        job: &ClaimedJob,
        now: i64,
        error_code: Option<&str>,
    ) -> Result<(), AppError> {
        if error_code.is_some_and(|code| {
            code.len() > 64
                || !code
                    .bytes()
                    .all(|character| character.is_ascii_lowercase() || character == b'_')
        }) {
            return Err(AppError::Invalid("Job error must be a safe error code"));
        }
        let status = if error_code.is_some() {
            "failed"
        } else {
            "succeeded"
        };
        let updated = sqlx::query(queries::FINISH_JOB)
            .bind(status)
            .bind(error_code)
            .bind(&job.job_id)
            .bind(&job.lease_token)
            .bind(now)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict("Job lease is no longer owned"));
        }
        Ok(())
    }
}
