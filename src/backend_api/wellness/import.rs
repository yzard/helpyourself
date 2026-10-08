//! Open GPX 1.1 tracks. Original UTF-8 text remains in the replayable health envelope.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::{ConnectionRequest, HealthRecordInput, SyncRequest},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XmlImportRequest {
    pub filename: String,
    pub xml: String,
}
const GPX_NS: &str = "http://www.topografix.com/GPX/1/1";

pub fn parse(request: &XmlImportRequest) -> Result<HealthRecordInput, AppError> {
    if request.xml.len() > 4 * 1024 * 1024
        || request.filename.is_empty()
        || request.filename.len() > 255
        || request.filename.contains(['/', '\\'])
    {
        return Err(AppError::Invalid(
            "A GPX filename and at most 4 MiB of UTF-8 XML are required",
        ));
    }
    let document = roxmltree::Document::parse_with_options(
        &request.xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 200000,
            ..Default::default()
        },
    )
    .map_err(|_| AppError::Invalid("Invalid GPX XML; DTDs are not supported"))?;
    let root = document.root_element();
    if !root.has_tag_name((GPX_NS, "gpx")) || root.attribute("version") != Some("1.1") {
        return Err(AppError::Invalid(
            "GPX 1.1 with its standard namespace is required",
        ));
    }
    let mut count = 0;
    let mut first: Option<i64> = None;
    let mut last: Option<i64> = None;
    let mut meters = 0.0;
    let mut segment_count = 0;
    for track in root.children().filter(|n| n.has_tag_name((GPX_NS, "trk"))) {
        for segment in track
            .children()
            .filter(|n| n.has_tag_name((GPX_NS, "trkseg")))
        {
            segment_count += 1;
            let mut previous: Option<(f64, f64)> = None;
            for point in segment
                .children()
                .filter(|n| n.has_tag_name((GPX_NS, "trkpt")))
            {
                count += 1;
                if count > 50000 {
                    return Err(AppError::Invalid("GPX has more than 50000 track points"));
                }
                let number = |name| {
                    point
                        .attribute(name)
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                        .ok_or(AppError::Invalid("Invalid GPX coordinate"))
                };
                let latitude = number("lat")?;
                let longitude = number("lon")?;
                if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
                    return Err(AppError::Invalid("GPX coordinate is out of range"));
                }
                let time = point
                    .children()
                    .find(|n| n.has_tag_name((GPX_NS, "time")))
                    .and_then(|n| n.text())
                    .ok_or(AppError::Invalid("Every track point needs a timestamp"))?;
                let at = chrono::DateTime::parse_from_rfc3339(time)
                    .map_err(|_| AppError::Invalid("Invalid GPX timestamp"))?
                    .timestamp();
                if last.is_some_and(|v| at <= v) {
                    return Err(AppError::Invalid(
                        "GPX timestamps must increase across tracks",
                    ));
                }
                first.get_or_insert(at);
                last = Some(at);
                if let Some((lat, lon)) = previous {
                    meters += distance(lat, lon, latitude, longitude);
                }
                previous = Some((latitude, longitude));
            }
        }
    }
    let start = first.ok_or(AppError::Invalid("GPX contains no timed track points"))?;
    let end = last.ok_or(AppError::Internal)?;
    if count < 2 || i128::from(end) - i128::from(start) > 7 * 86400 {
        return Err(AppError::Invalid(
            "A track needs at least two points within seven days",
        ));
    }
    let hash = hex::encode(Sha256::digest(request.xml.as_bytes()));
    Ok(HealthRecordInput {
        record_id: hash.clone(),
        source_id: "gpx-1.1".into(),
        record_type: "workout".into(),
        start_at: start,
        end_at: end,
        version: 1,
        deleted: false,
        payload: json!({"format":"gpx-1.1", "filename":request.filename, "original_utf8":request.xml,"sha256":hash,"track_points":count,"segments":segment_count,"distance_m":meters,"elapsed_seconds":end-start,"algorithm_version":"gpx-track-v1","duration_semantics":"elapsed_including_gaps_not_active_time","distance_semantics":"spherical_distance_within_segments_no_elevation_correction"}),
    })
}
fn distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let a = ((lat2 - lat1).to_radians() / 2.0).sin().powi(2)
        + lat1.to_radians().cos()
            * lat2.to_radians().cos()
            * ((lon2 - lon1).to_radians() / 2.0).sin().powi(2);
    2.0 * 6371008.8 * a.clamp(0.0, 1.0).sqrt().asin()
}
impl Database {
    pub async fn import_gpx(
        &self,
        user: &str,
        request: XmlImportRequest,
    ) -> Result<Value, AppError> {
        let record = self.cpu.run(move || parse(&request)).await?;
        self.import_training_record(user, record).await
    }
    pub async fn import_tcx(
        &self,
        user: &str,
        request: XmlImportRequest,
    ) -> Result<Value, AppError> {
        let record = self.cpu.run(move || parse_tcx(&request)).await?;
        self.import_training_record(user, record).await
    }
    pub(super) async fn import_training_record(
        &self,
        user: &str,
        record: HealthRecordInput,
    ) -> Result<Value, AppError> {
        let source_id = record.source_id.clone();
        let record_id = record.record_id.clone();
        if let Some(previous) = sqlx::query(queries::HEALTH_RECORD)
            .bind(user)
            .bind("file_import")
            .bind(&source_id)
            .bind(&record_id)
            .fetch_optional(&self.pool)
            .await?
        {
            if previous.get::<i64, _>("deleted") == 1 {
                return Err(AppError::Conflict(
                    "This original was deleted and cannot be restored by reimporting",
                ));
            }
            return Ok(
                json!({"record_id":record_id,"source":format!("file_import:{source_id}"),"duplicate":true}),
            );
        }

        let connection = self
            .create_connection(
                user,
                ConnectionRequest {
                    platform: "file_import".into(),
                    installation_id: "f12dbdbe-3e6b-4ac4-9f10-a8f9fb27ae05".into(),
                },
            )
            .await?;
        let result = self
            .sync_health(
                user,
                SyncRequest {
                    connection_id: connection.connection_id,
                    batch_id: uuid::Uuid::new_v4().to_string(),
                    record_type: "workout".into(),
                    coverage_status: "observed".into(),
                    records: vec![record],
                },
            )
            .await?;
        Ok(json!({"record_id":record_id,"source":format!("file_import:{source_id}"),"sync":result}))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportListRequest {
    pub after_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDeleteRequest {
    pub source_id: String,
    pub record_id: String,
    pub expected_version: i64,
}
impl Database {
    pub async fn list_training_imports(
        &self,
        user: &str,
        request: ImportListRequest,
    ) -> Result<Value, AppError> {
        if request
            .after_id
            .as_ref()
            .is_some_and(|id| id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(AppError::Invalid("Invalid import cursor"));
        }
        let rows = sqlx::query(queries::TRAINING_IMPORTS)
            .bind(user)
            .bind(&request.after_id)
            .bind(&request.after_id)
            .fetch_all(&self.pool)
            .await?;
        let more = rows.len() > 100;
        let imports:Vec<_>=rows.into_iter().take(100).map(|r| json!({"source_id":r.get::<String,_>("source_id"),"record_id":r.get::<String,_>("record_id"),"start_at":r.get::<i64,_>("start_at"),"end_at":r.get::<i64,_>("end_at"),"version":r.get::<i64,_>("version"),"filename":r.get::<String,_>("filename"),"distance_m":r.get::<Option<f64>,_>("distance_m")})).collect();
        Ok(
            json!({"next_after_id":if more {imports.last().map(|r|r["record_id"].clone())} else {None},"imports":imports}),
        )
    }
    pub async fn delete_training_import(
        &self,
        user: &str,
        request: ImportDeleteRequest,
    ) -> Result<Value, AppError> {
        if !["gpx-1.1", "tcx-2", "fit"].contains(&request.source_id.as_str()) {
            return Err(AppError::Invalid("Unknown training file source"));
        }
        let row = sqlx::query(queries::HEALTH_RECORD)
            .bind(user)
            .bind("file_import")
            .bind(&request.source_id)
            .bind(&request.record_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        if row.get::<i64, _>("deleted") == 1
            || row.get::<i64, _>("version") != request.expected_version
        {
            return Err(AppError::Conflict(
                "Import changed; refresh before deleting",
            ));
        }
        let connection = self
            .create_connection(
                user,
                ConnectionRequest {
                    platform: "file_import".into(),
                    installation_id: "f12dbdbe-3e6b-4ac4-9f10-a8f9fb27ae05".into(),
                },
            )
            .await?;
        self.sync_health(
            user,
            SyncRequest {
                connection_id: connection.connection_id,
                batch_id: uuid::Uuid::new_v4().to_string(),
                record_type: "workout".into(),
                coverage_status: "observed".into(),
                records: vec![HealthRecordInput {
                    record_id: request.record_id,
                    source_id: request.source_id,
                    record_type: "workout".into(),
                    start_at: 0,
                    end_at: 0,
                    version: request
                        .expected_version
                        .checked_add(1)
                        .ok_or(AppError::Invalid("Version overflow"))?,
                    deleted: true,
                    payload: json!({}),
                }],
            },
        )
        .await
    }
}

const TCX_NS: &str = "http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2";
fn tcx_child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|n| n.has_tag_name((TCX_NS, name)))
}
fn tcx_number(
    node: roxmltree::Node<'_, '_>,
    name: &str,
    lower: f64,
    upper: f64,
) -> Result<Option<f64>, AppError> {
    tcx_child(node, name)
        .map(|n| {
            n.text()
                .and_then(|t| t.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= lower && *v <= upper)
                .ok_or(AppError::Invalid("Invalid TCX numeric field"))
        })
        .transpose()
}
fn tcx_time(text: Option<&str>) -> Result<i64, AppError> {
    chrono::DateTime::parse_from_rfc3339(text.ok_or(AppError::Invalid("TCX timestamp is missing"))?)
        .map(|d| d.timestamp())
        .map_err(|_| AppError::Invalid("Invalid TCX timestamp"))
}
pub fn parse_tcx(request: &XmlImportRequest) -> Result<HealthRecordInput, AppError> {
    if request.xml.len() > 4 * 1024 * 1024
        || request.filename.is_empty()
        || request.filename.len() > 255
        || request.filename.contains(['/', '\\'])
    {
        return Err(AppError::Invalid(
            "A filename and at most 4 MiB of UTF-8 TCX are required",
        ));
    }
    let document = roxmltree::Document::parse_with_options(
        &request.xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 200000,
            ..Default::default()
        },
    )
    .map_err(|_| AppError::Invalid("Invalid TCX XML; DTDs are not supported"))?;
    let root = document.root_element();
    if !root.has_tag_name((TCX_NS, "TrainingCenterDatabase")) {
        return Err(AppError::Invalid("TCX version 2 namespace is required"));
    }
    let activities =
        tcx_child(root, "Activities").ok_or(AppError::Invalid("TCX Activities are required"))?;
    let activities: Vec<_> = activities
        .children()
        .filter(|n| n.has_tag_name((TCX_NS, "Activity")))
        .collect();
    if activities.len() != 1 {
        return Err(AppError::Invalid("Import one TCX activity per file"));
    }
    let activity = activities[0];
    let sport = activity
        .attribute("Sport")
        .filter(|s| ["Running", "Biking", "Other"].contains(s))
        .ok_or(AppError::Invalid("Unsupported TCX sport"))?;
    let mut laps = Vec::new();
    let mut first = None;
    let mut previous_end = None;
    let mut last_point = None;
    let mut total_distance = 0.0;
    let mut total_time = 0.0;
    let mut count = 0;
    for lap in activity
        .children()
        .filter(|n| n.has_tag_name((TCX_NS, "Lap")))
    {
        if laps.len() >= 1000 {
            return Err(AppError::TooLarge);
        }
        let start = tcx_time(lap.attribute("StartTime"))?;
        if previous_end.is_some_and(|end| start < end) {
            return Err(AppError::Invalid("TCX laps overlap or run backward"));
        }
        first.get_or_insert(start);
        let duration = tcx_number(lap, "TotalTimeSeconds", 0.0, 604800.0)?
            .ok_or(AppError::Invalid("TCX lap time is missing"))?;
        let meters = tcx_number(lap, "DistanceMeters", 0.0, 10000000.0)?
            .ok_or(AppError::Invalid("TCX lap distance is missing"))?;
        let mut end = start
            .checked_add(duration.ceil() as i64)
            .ok_or(AppError::Invalid("TCX time overflow"))?;
        let mut tracks = Vec::new();
        for track in lap.children().filter(|n| n.has_tag_name((TCX_NS, "Track"))) {
            let mut points = Vec::new();
            for point in track
                .children()
                .filter(|n| n.has_tag_name((TCX_NS, "Trackpoint")))
            {
                count += 1;
                if count > 50000 {
                    return Err(AppError::TooLarge);
                }
                let at = tcx_time(tcx_child(point, "Time").and_then(|n| n.text()))?;
                if at < start || last_point.is_some_and(|last| at <= last) {
                    return Err(AppError::Invalid("TCX point timestamps must increase"));
                }
                last_point = Some(at);
                end = end.max(at);
                let position = if let Some(position) = tcx_child(point, "Position") {
                    let lat = tcx_number(position, "LatitudeDegrees", -90.0, 90.0)?
                        .ok_or(AppError::Invalid("TCX latitude is missing"))?;
                    let lon = tcx_number(position, "LongitudeDegrees", -180.0, 180.0)?
                        .ok_or(AppError::Invalid("TCX longitude is missing"))?;
                    json!({"latitude":lat,"longitude":lon})
                } else {
                    Value::Null
                };
                let hr = tcx_child(point, "HeartRateBpm")
                    .map(|n| tcx_number(n, "Value", 1.0, 400.0))
                    .transpose()?
                    .flatten();
                points.push(json!({"at":at,"position":position,"heart_rate_bpm":hr,"cadence_rpm":tcx_number(point,"Cadence",0.0,254.0)?,"altitude_m":tcx_number(point,"AltitudeMeters",-12000.0,100000.0)?,"distance_m":tcx_number(point,"DistanceMeters",0.0,10000000.0)?}));
            }
            tracks.push(points);
        }
        total_distance += meters;
        total_time += duration;
        previous_end = Some(end);
        laps.push(json!({"start_at":start,"end_at":end,"source_total_time_seconds":duration,"source_distance_m":meters,"tracks":tracks}));
    }
    let start = first.ok_or(AppError::Invalid("TCX activity has no laps"))?;
    let end = previous_end.ok_or(AppError::Internal)?;
    if end <= start || i128::from(end) - i128::from(start) > 604800 || total_time > 604800.0 {
        return Err(AppError::Invalid("TCX activity must fit within seven days"));
    }
    let hash = hex::encode(Sha256::digest(request.xml.as_bytes()));
    Ok(HealthRecordInput {
        record_id: hash.clone(),
        source_id: "tcx-2".into(),
        record_type: "workout".into(),
        start_at: start,
        end_at: end,
        version: 1,
        deleted: false,
        payload: json!({"format":"tcx-2","filename":request.filename,"original_utf8":request.xml,"sha256":hash,"sport":sport,"laps":laps,"track_points":count,"distance_m":total_distance,"source_total_time_seconds":total_time,"elapsed_seconds":end-start,"algorithm_version":"tcx-activity-v1","distance_semantics":"sum_of_source_lap_distances","duration_semantics":"source_lap_timer_separate_from_elapsed_window","extensions":"preserved_in_original_xml"}),
    })
}
