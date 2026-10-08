//! A bounded timeline of archive events. Report timestamps mean upload time.
use crate::{
    database::{Database, queries},
    error::AppError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub at: i64,
    pub key: String,
    pub data_revision: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineRequest {
    pub start_at: i64,
    pub end_at: i64,
    pub cursor: Option<Cursor>,
}
impl Database {
    pub async fn wellness_timeline(
        &self,
        user: &str,
        request: TimelineRequest,
    ) -> Result<Value, AppError> {
        if request.end_at <= request.start_at
            || i128::from(request.end_at) - i128::from(request.start_at) > 31 * 86400
            || request.cursor.as_ref().is_some_and(|c| {
                c.key.len() > 4096 || c.at < request.start_at || c.at >= request.end_at
            })
        {
            return Err(AppError::Invalid(
                "Select a timeline window of at most 31 days",
            ));
        }
        let revision = self.data_revision(user).await?;
        if request
            .cursor
            .as_ref()
            .is_some_and(|c| c.data_revision != revision)
        {
            return Err(AppError::Conflict("Archive changed; restart the timeline"));
        }
        let at = request.cursor.as_ref().map(|c| c.at);
        let key = request.cursor.as_ref().map(|c| c.key.as_str());
        let rows = sqlx::query(queries::WELLNESS_TIMELINE)
            .bind(user)
            .bind(request.start_at)
            .bind(request.end_at)
            .bind(user)
            .bind(request.start_at)
            .bind(request.end_at)
            .bind(at)
            .bind(at)
            .bind(at)
            .bind(key)
            .fetch_all(&self.pool)
            .await?;
        let more = rows.len() > 200;
        let mut events = Vec::new();
        let mut next = None;
        for row in rows.into_iter().take(200) {
            let at: i64 = row.get("event_at");
            let key: String = row.get("event_key");
            next = Some(Cursor {
                at,
                key: key.clone(),
                data_revision: revision,
            });
            events.push(json!({"at":at,"key":key,"record_id":row.get::<String,_>("record_id"),"record_type":row.get::<String,_>("record_type"),"source":row.get::<String,_>("source"),"version":row.get::<i64,_>("version"),"time_semantics":row.get::<String,_>("time_semantics")}));
        }
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; restart the timeline"));
        }
        Ok(
            json!({"events":events,"next_cursor":if more {next} else {None},"data_revision":revision,"start_at":request.start_at,"end_at":request.end_at,"scope":"reports_uploaded_and_sleep_workout_manual_events","notes":["Report times are upload times, not specimen collection times.","Sleep stages are original intervals, not separate nights. Device measurements are available in Device trends."]}),
        )
    }
}
