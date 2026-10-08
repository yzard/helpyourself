use helpyourself::wellness::{DayRequest, algorithms::*, select_metric};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn independent_sources_require_a_choice_and_baselines_never_mix_sources() {
    let data = json!({"algorithm_version":"fixture","days":[
        {"date":"2026-10-06","source":"apple_health:watch","value":600,"unit":"seconds"},
        {"date":"2026-10-06","source":"health_connect:ring","value":900,"unit":"seconds"}]});
    let none = select_metric("sleep", "2026-10-06", &[], &data).unwrap();
    assert_eq!(none["current"]["state"], "source_selection_required");
    assert!(none["current"]["selected"].is_null());
    let selected = select_metric(
        "sleep",
        "2026-10-06",
        &["health_connect:ring".into()],
        &data,
    )
    .unwrap();
    assert_eq!(selected["current"]["selected"]["value"], 900);
    assert_eq!(
        selected["current"]["alternatives"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(selected["baseline_valid_days"], 0);
    let unavailable = select_metric("sleep", "2026-10-06", &["missing".into()], &data).unwrap();
    assert!(unavailable["current"]["selected"].is_null());
}

#[test]
fn formulas_preserve_units_and_reject_missing_or_invalid_inputs() {
    assert_eq!(session_load(6.0, 45.0).unwrap(), 270.0);
    assert!(session_load(11.0, 45.0).is_err());
    assert_eq!(rmssd(&[800.0, 900.0, 800.0]).unwrap(), 100.0);
    assert!(rmssd(&[800.0, f64::NAN]).is_err());
    let mut states: Vec<Option<bool>> = (0..10080).map(|m| Some(m % 1440 < 480)).collect();
    assert_eq!(sleep_regularity(&states).unwrap(), 100.0);
    states[1440] = None;
    assert!(sleep_regularity(&states).is_err());
    let summary = glucose(
        &[
            (0, 50.0),
            (300, 100.0),
            (600, 200.0),
            (900, 100.0),
            (3600, 100.0),
        ],
        0,
        3600,
        300,
    )
    .unwrap();
    assert_eq!(summary.covered_seconds, 900);
    assert_eq!(summary.coverage_fraction, 0.25);
    assert!((summary.tir_percent.unwrap() - 100.0 / 3.0).abs() < 1e-9);
    assert!((summary.below_54_percent.unwrap() - 100.0 / 3.0).abs() < 1e-9);
    assert!(glucose(&[(0, 100.0), (0, 120.0)], 0, 3600, 300).is_err());
    assert!(glucose(&[], 0, 3600, 300).unwrap().mean_mg_dl.is_none());
}

#[tokio::test]
async fn daily_view_validates_dates_priorities_and_user_boundaries() {
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "alice").await;
    let data = state
        .database
        .wellness_day(
            &user.user_id,
            DayRequest {
                date: "2026-10-06".into(),
                timezone: "America/New_York".into(),
                source_priority: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(data["metrics"].as_array().unwrap().len(), 6);
    assert!(
        data["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["current"]["state"] == "insufficient_data")
    );
    assert!(
        state
            .database
            .wellness_day(
                &user.user_id,
                DayRequest {
                    date: "bad".into(),
                    timezone: "UTC".into(),
                    source_priority: BTreeMap::new()
                }
            )
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .wellness_day(
                "other",
                DayRequest {
                    date: "2026-10-06".into(),
                    timezone: "UTC".into(),
                    source_priority: BTreeMap::new()
                }
            )
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .wellness_day(
                &user.user_id,
                DayRequest {
                    date: "2026-10-06".into(),
                    timezone: "invalid".into(),
                    source_priority: BTreeMap::new()
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn manual_entries_round_trip_revise_isolate_and_delete() {
    use helpyourself::wellness::entries::{DeleteRequest, ListRequest, SaveRequest};
    let (_directory, state) = crate::fixture().await;
    let alice = crate::add_user(&state, "manual-alice").await;
    let bob = crate::add_user(&state, "manual-bob").await;
    let id = uuid::Uuid::new_v4().to_string();
    let payload = json!({"record_id":id,"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":{"kind":"training","content":{"activity":"Strength","duration_minutes":45,"ended_at":2800,"paused_minutes":0,"duration_basis":"elapsed_including_pauses","rpe_answered_at":4600,"rpe_cr10":6,"sets":[{"exercise":"Squat","repetitions":5,"external_weight_kg":60}],"note":"synthetic"}}});
    let save = || serde_json::from_value::<SaveRequest>(payload.clone()).unwrap();
    state
        .database
        .save_wellness_entry(&alice.user_id, save())
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .save_wellness_entry(&alice.user_id, save())
            .await
            .unwrap()["sync"]["replayed"],
        true
    );
    let list = || ListRequest {
        start_at: 0,
        end_at: 200,
        kind: None,
    };
    let stored = state
        .database
        .list_wellness_entries(&alice.user_id, list())
        .await
        .unwrap();
    assert_eq!(
        stored["entries"][0]["calculation"]["session_load_au"],
        270.0
    );
    assert_eq!(
        stored["entries"][0]["calculation"]["external_volume_kg_repetitions_by_exercise"]["Squat"],
        300.0
    );
    assert!(
        state
            .database
            .list_wellness_entries(&bob.user_id, list())
            .await
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut revised = payload.clone();
    revised["version"] = json!(2);
    revised["batch_id"] = json!(uuid::Uuid::new_v4());
    revised["entry"]["content"]["rpe_cr10"] = json!(8);
    state
        .database
        .save_wellness_entry(&alice.user_id, serde_json::from_value(revised).unwrap())
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .list_wellness_entries(&alice.user_id, list())
            .await
            .unwrap()["entries"][0]["calculation"]["session_load_au"],
        360.0
    );
    state
        .database
        .delete_wellness_entry(
            &alice.user_id,
            DeleteRequest {
                record_id: id,
                version: 3,
                batch_id: uuid::Uuid::new_v4().to_string(),
                kind: "training".into(),
            },
        )
        .await
        .unwrap();
    assert!(
        state
            .database
            .list_wellness_entries(&alice.user_id, list())
            .await
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut late = payload;
    late["version"] = json!(4);
    late["batch_id"] = json!(uuid::Uuid::new_v4());
    assert!(
        state
            .database
            .save_wellness_entry(&alice.user_id, serde_json::from_value(late).unwrap())
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .list_wellness_entries(&alice.user_id, list())
            .await
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn gpx_preserves_original_and_never_bridges_segments() {
    use helpyourself::wellness::import::{XmlImportRequest, parse};
    let xml = r#"<?xml version="1.0"?><gpx version="1.1" creator="synthetic" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg><trkpt lat="0" lon="0"><time>2026-01-01T00:00:00Z</time></trkpt><trkpt lat="0" lon="0.001"><time>2026-01-01T00:01:00Z</time></trkpt></trkseg><trkseg><trkpt lat="1" lon="1"><time>2026-01-01T00:02:00Z</time></trkpt></trkseg></trk></gpx>"#;
    let record = parse(&XmlImportRequest {
        filename: "test.gpx".into(),
        xml: xml.into(),
    })
    .unwrap();
    assert_eq!(record.payload["original_utf8"], xml);
    assert_eq!(record.end_at - record.start_at, 120);
    assert!((record.payload["distance_m"].as_f64().unwrap() - 111.195).abs() < 0.01);
    assert!(
        parse(&XmlImportRequest {
            filename: "test.gpx".into(),
            xml: xml.replace("00:02:00", "00:00:00")
        })
        .is_err()
    );
    assert!(
        parse(&XmlImportRequest {
            filename: "test.gpx".into(),
            xml: xml.replace("lat=\"1\"", "lat=\"91\"")
        })
        .is_err()
    );
    assert!(
        parse(&XmlImportRequest {
            filename: "test.gpx".into(),
            xml: "<!DOCTYPE gpx [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><gpx/>".into()
        })
        .is_err()
    );
}

#[tokio::test]
async fn reserved_connections_cannot_bypass_domain_validation() {
    use axum::http::StatusCode;
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "reserved").await;
    let token = crate::token(&state, "reserved").await;
    let app = helpyourself::app::create_application(state.clone());
    for endpoint in [
        "wellness/day",
        "wellness/sources",
        "wellness/series",
        "wellness/timeline",
        "wellness/review",
        "wellness/preferences/get",
        "wellness/preferences/save",
        "wellness/entries/list",
        "wellness/entries/save",
        "wellness/entries/delete",
        "wellness/import/gpx",
    ] {
        assert_eq!(
            crate::request(&app, &format!("/api/v1/{endpoint}"), None, json!({}))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        crate::request(
            &app,
            "/api/v1/health/connect",
            Some(&token),
            json!({"platform":"manual","installation_id":uuid::Uuid::new_v4()})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            helpyourself::health::ConnectionRequest {
                platform: "manual".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    assert_eq!(crate::request(&app,"/api/v1/health/sync",Some(&token),json!({"connection_id":connection.connection_id,"batch_id":uuid::Uuid::new_v4(),"record_type":"journal","coverage_status":"observed","records":[]})).await.0,StatusCode::BAD_REQUEST);
}

#[test]
fn sleep_preserves_gaps_conflicts_and_in_bed_denominator() {
    use helpyourself::{health::StoredHealthRecord, wellness::sleep::summarize};
    let record = |id: &str, start, end, category| StoredHealthRecord {
        platform: "apple_health".into(),
        source_id: "watch".into(),
        record_id: id.into(),
        record_type: "sleep".into(),
        start_at: start,
        end_at: end,
        version: 1,
        payload_json: json!({"payload":{"category":category}}).to_string(),
    };
    let out = summarize(
        vec![
            record("bed", 0, 1000, 0),
            record("sleep", 100, 500, 3),
            record("rem", 600, 900, 5),
        ],
        0,
        1001,
        chrono_tz::UTC,
    )
    .unwrap();
    let session = &out["sources"][0]["sessions"][0];
    assert_eq!(session["asleep_seconds"], 700);
    assert_eq!(session["observed_awake_seconds"], 0);
    assert_eq!(session["efficiency"], 0.7);
    let no_bed = summarize(vec![record("sleep", 100, 500, 3)], 0, 1001, chrono_tz::UTC).unwrap();
    assert!(no_bed["sources"][0]["sessions"][0]["efficiency"].is_null());
    let conflict = summarize(
        vec![record("sleep", 100, 500, 3), record("awake", 400, 600, 2)],
        0,
        1001,
        chrono_tz::UTC,
    )
    .unwrap();
    assert_eq!(
        conflict["sources"][0]["sessions"][0]["state"],
        "conflicting_states"
    );
    assert!(conflict["sources"][0]["sessions"][0]["asleep_seconds"].is_null());
    assert!(
        summarize(
            vec![record("overflow", i64::MIN, i64::MAX, 1)],
            0,
            1001,
            chrono_tz::UTC
        )
        .is_err()
    );
}

#[test]
fn clinical_age_matches_independent_probability_space_reference() {
    let input = ClinicalAge {
        age_years: 50.0,
        albumin_g_l: 45.0,
        creatinine_umol_l: 80.0,
        glucose_mmol_l: 5.0,
        crp_mg_dl: 0.1,
        lymphocyte_percent: 30.0,
        mcv_fl: 90.0,
        rdw_percent: 13.0,
        alp_u_l: 60.0,
        wbc_1000_ul: 5.0,
    };
    let value = input.calculate().unwrap();
    assert!((value - 42.69123135792742).abs() < 1e-10);
}

#[tokio::test]
async fn clinical_model_requires_complete_reviewed_collection_and_explicit_research_use() {
    use helpyourself::{
        reports::{ReviewItem, ReviewRequest},
        wellness::clinical::ClinicalRequest,
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "clinical").await;
    let other = crate::add_user(&state, "clinical-other").await;
    let report = crate::archive(&state, "clinical").await;
    let request = || ClinicalRequest {
        report_id: report.clone(),
        age_at_collection_years: 50.0,
        research_acknowledged: true,
    };
    let missing = state
        .database
        .clinical_age(&user.user_id, request())
        .await
        .unwrap();
    assert_eq!(missing["missing_metrics"].as_array().unwrap().len(), 9);
    assert!(
        state
            .database
            .clinical_age(&other.user_id, request())
            .await
            .is_err()
    );
    let mut denied = request();
    denied.research_acknowledged = false;
    assert!(
        state
            .database
            .clinical_age(&user.user_id, denied)
            .await
            .is_err()
    );
    let samples = [
        ("albumin", "45", "g/L"),
        ("creatinine", "80", "µmol/L"),
        ("glucose", "5", "mmol/L"),
        ("crp", "1", "mg/L"),
        ("lymphocyte_percent", "30", "%"),
        ("mcv", "90", "fL"),
        ("rdw", "13", "%"),
        ("alp", "60", "U/L"),
        ("wbc", "5", "10^9/L"),
    ];
    let observations = samples
        .into_iter()
        .map(|(metric, value, unit)| {
            let mut payload = crate::observation_payload();
            payload.metric_id = Some(metric.into());
            payload.raw_name = metric.into();
            payload.raw_result = value.into();
            payload.raw_unit = Some(unit.into());
            ReviewItem {
                observation_id: None,
                expected_revision: None,
                status: "confirmed".into(),
                payload,
            }
        })
        .collect();
    let revision = state
        .database
        .report_summary(&user.user_id, &report)
        .await
        .unwrap()
        .revision;
    state
        .database
        .review(
            &user.user_id,
            ReviewRequest {
                report_id: report.clone(),
                expected_revision: revision,
                context: None,
                observations,
            },
        )
        .await
        .unwrap();
    let result = state
        .database
        .clinical_age(&user.user_id, request())
        .await
        .unwrap();
    assert!((result["value"].as_f64().unwrap() - 42.69123135792742).abs() < 0.0001);
    assert_eq!(result["inputs"].as_array().unwrap().len(), 9);
    assert_eq!(result["evidence_status"], "research");
}

#[tokio::test]
async fn gpx_deduplicates_renamed_files_isolates_users_and_deletes_originals() {
    use helpyourself::wellness::import::{
        ImportDeleteRequest, ImportListRequest, XmlImportRequest,
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "gpx").await;
    let other = crate::add_user(&state, "gpx-other").await;
    let xml = "\u{feff}<gpx version=\"1.1\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><trkseg><trkpt lat=\"0\" lon=\"0\"><time>2026-01-01T00:00:00Z</time></trkpt><trkpt lat=\"0\" lon=\"0.001\"><time>2026-01-01T00:01:00Z</time></trkpt></trkseg></trk></gpx>";
    let request = |name: &str| XmlImportRequest {
        filename: name.into(),
        xml: xml.into(),
    };
    let first = state
        .database
        .import_gpx(&user.user_id, request("original.gpx"))
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .import_gpx(&user.user_id, request("renamed.gpx"))
            .await
            .unwrap()["duplicate"],
        true
    );
    let rows = state
        .database
        .list_training_imports(&user.user_id, ImportListRequest { after_id: None })
        .await
        .unwrap();
    assert_eq!(rows["imports"].as_array().unwrap().len(), 1);
    assert_eq!(rows["imports"][0]["filename"], "original.gpx");
    assert!(
        state
            .database
            .list_training_imports(&other.user_id, ImportListRequest { after_id: None })
            .await
            .unwrap()["imports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let id = first["record_id"].as_str().unwrap().to_string();
    assert!(
        state
            .database
            .delete_training_import(
                &other.user_id,
                ImportDeleteRequest {
                    source_id: "gpx-1.1".into(),
                    record_id: id.clone(),
                    expected_version: 1
                }
            )
            .await
            .is_err()
    );
    state
        .database
        .delete_training_import(
            &user.user_id,
            ImportDeleteRequest {
                source_id: "gpx-1.1".into(),
                record_id: id,
                expected_version: 1,
            },
        )
        .await
        .unwrap();
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    assert!(
        state
            .database
            .list_training_imports(&user.user_id, ImportListRequest { after_id: None })
            .await
            .unwrap()["imports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .database
            .import_gpx(&user.user_id, request("original.gpx"))
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_dir(
            state
                .config
                .data_dir
                .join("raw/file_import")
                .join(&user.user_id)
        )
        .unwrap()
        .count(),
        0
    );
}

#[tokio::test]
async fn preferences_are_versioned_isolated_and_preserve_source_fallbacks() {
    let (_directory, state) = crate::fixture().await;
    let alice = crate::add_user(&state, "prefs-alice").await;
    let bob = crate::add_user(&state, "prefs-bob").await;
    assert_eq!(
        state
            .database
            .wellness_preferences(&alice.user_id)
            .await
            .unwrap()["version"],
        0
    );
    let input = json!({"expected_version":0,"batch_id":uuid::Uuid::new_v4(),"preferences":{"source_priority":{"sleep":["apple_health:watch","health_connect:ring"]},"favorite_metrics":["sleep"],"sleep_target_minutes":480}});
    let result = state
        .database
        .save_wellness_preferences(
            &alice.user_id,
            serde_json::from_value(input.clone()).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(result["version"], 1);
    assert_eq!(result["preferences"], input["preferences"]);
    let mut stale = input.clone();
    stale["batch_id"] = json!(uuid::Uuid::new_v4());
    stale["preferences"]["sleep_target_minutes"] = json!(500);
    assert!(
        state
            .database
            .save_wellness_preferences(
                &alice.user_id,
                serde_json::from_value(stale.clone()).unwrap()
            )
            .await
            .is_err()
    );
    stale["expected_version"] = json!(1);
    assert_eq!(
        state
            .database
            .save_wellness_preferences(&alice.user_id, serde_json::from_value(stale).unwrap())
            .await
            .unwrap()["version"],
        2
    );
    assert_eq!(
        state
            .database
            .wellness_preferences(&bob.user_id)
            .await
            .unwrap()["version"],
        0
    );
    let mut invalid = input;
    invalid["preferences"]["favorite_metrics"] = json!(["sleep", "sleep"]);
    assert!(
        state
            .database
            .save_wellness_preferences(&bob.user_id, serde_json::from_value(invalid).unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn device_series_keeps_sources_and_uses_actual_healthkit_percentage_units() {
    use helpyourself::{
        health::{ConnectionRequest, HealthRecordInput, SyncRequest},
        wellness::series::SeriesRequest,
    };
    let (_directory, state) = crate::fixture().await;
    let alice = crate::add_user(&state, "series-alice").await;
    let bob = crate::add_user(&state, "series-bob").await;
    let connection = state
        .database
        .create_connection(
            &alice.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    for (kind, unit, samples) in [
        (
            "blood_glucose",
            "mg/dL",
            vec![(0, 50.0), (300, 100.0), (600, 200.0)],
        ),
        ("oxygen_saturation", "%", vec![(0, 0.97)]),
        (
            "heart_rate",
            "count/min",
            vec![(0, 100.0), (10, 120.0), (20, 140.0)],
        ),
    ] {
        state
            .database
            .sync_health(
                &alice.user_id,
                SyncRequest {
                    connection_id: connection.connection_id.clone(),
                    batch_id: uuid::Uuid::new_v4().to_string(),
                    record_type: kind.into(),
                    coverage_status: "observed".into(),
                    records: samples
                        .into_iter()
                        .map(|(at, value)| HealthRecordInput {
                            record_id: format!("{kind}-{at}"),
                            source_id: "sensor".into(),
                            record_type: kind.into(),
                            start_at: at,
                            end_at: at,
                            version: 1,
                            deleted: false,
                            payload: json!({"value":if kind=="heart_rate" {json!(value as i64)}else{json!(value)},"unit":unit}),
                        })
                        .collect(),
                },
            )
            .await
            .unwrap();
    }
    let request = |kind: &str| SeriesRequest {
        declared_maximum: None,
        record_type: kind.into(),
        start_at: 0,
        end_at: 900,
        maximum_gap_seconds: 300,
    };
    let heart = state
        .database
        .wellness_series(&alice.user_id, request("heart_rate"))
        .await
        .unwrap();
    assert_eq!(
        heart["sources"][0]["heart_rate_summary"]["covered_seconds"],
        20
    );
    let glucose = state
        .database
        .wellness_series(&alice.user_id, request("blood_glucose"))
        .await
        .unwrap();
    assert_eq!(
        glucose["sources"][0]["glucose_summary"]["covered_seconds"],
        600
    );
    assert_eq!(
        glucose["sources"][0]["glucose_summary"]["tir_percent"],
        50.0
    );
    let oxygen = state
        .database
        .wellness_series(&alice.user_id, request("oxygen_saturation"))
        .await
        .unwrap();
    assert_eq!(oxygen["sources"][0]["points"][0]["value"], 97.0);
    assert_eq!(oxygen["sources"][0]["points"][0]["raw_value"], 0.97);
    assert!(
        state
            .database
            .wellness_series(&bob.user_id, request("blood_glucose"))
            .await
            .unwrap()["sources"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn review_keeps_missing_nutrition_unknown_and_observed_strength_separate() {
    use helpyourself::wellness::review::ReviewRequest;
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "review").await;
    for mut content in [
        json!({"food":"Breakfast","meal":"breakfast","energy_kcal":400,"protein_g":20,"carbohydrate_g":null,"fat_g":null,"fiber_g":null,"water_ml":null,"note":""}),
        json!({"food":"Lunch","meal":"lunch","energy_kcal":null,"protein_g":30,"carbohydrate_g":null,"fat_g":null,"fiber_g":null,"water_ml":null,"note":""}),
    ] {
        content["micronutrients"] = json!({});
        content["origin"] = json!(null);
        state.database.save_wellness_entry(&user.user_id,serde_json::from_value(json!({"record_id":uuid::Uuid::new_v4(),"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":{"kind":"nutrition","content":content}})).unwrap()).await.unwrap();
    }
    for weight in [60, 65] {
        state.database.save_wellness_entry(&user.user_id,serde_json::from_value(json!({"record_id":uuid::Uuid::new_v4(),"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":{"kind":"training","content":{"activity":"Strength","duration_minutes":40,"ended_at":2500,"paused_minutes":0,"duration_basis":"elapsed_including_pauses","rpe_answered_at":null,"rpe_cr10":null,"sets":[{"exercise":"Squat","repetitions":5,"external_weight_kg":weight}],"note":""}}})).unwrap()).await.unwrap();
    }
    let out = state
        .database
        .wellness_review(
            &user.user_id,
            ReviewRequest {
                start_at: 0,
                end_at: 200,
                timezone: "UTC".into(),
            },
        )
        .await
        .unwrap();
    let nutrition = &out["days"][0]["totals"];
    assert_eq!(nutrition[0]["observed_sum"], 400.0);
    assert_eq!(nutrition[0]["missing_count"], 1);
    assert_eq!(nutrition[1]["observed_sum"], 50.0);
    assert!(nutrition[2]["observed_sum"].is_null());
    assert!(out["days"][1]["totals"][1]["observed_sum"].is_null());
    assert_eq!(out["strength_bests"][0]["external_weight_kg"], 65.0);
}

#[tokio::test]
async fn timeline_pages_equal_timestamps_without_loss_and_rejects_stale_cursor() {
    use helpyourself::{
        health::{ConnectionRequest, HealthRecordInput, SyncRequest},
        wellness::timeline::{Cursor, TimelineRequest},
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "timeline").await;
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    state
        .database
        .sync_health(
            &user.user_id,
            SyncRequest {
                connection_id: connection.connection_id,
                batch_id: uuid::Uuid::new_v4().to_string(),
                record_type: "workout".into(),
                coverage_status: "observed".into(),
                records: (0..201)
                    .map(|i| HealthRecordInput {
                        record_id: format!("event-{i:03}"),
                        source_id: "watch".into(),
                        record_type: "workout".into(),
                        start_at: 100,
                        end_at: 150,
                        version: 1,
                        deleted: false,
                        payload: json!({}),
                    })
                    .collect(),
            },
        )
        .await
        .unwrap();
    let page = state
        .database
        .wellness_timeline(
            &user.user_id,
            TimelineRequest {
                start_at: 0,
                end_at: 200,
                cursor: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(page["events"].as_array().unwrap().len(), 200);
    let next = state
        .database
        .wellness_timeline(
            &user.user_id,
            TimelineRequest {
                start_at: 0,
                end_at: 200,
                cursor: Some(serde_json::from_value(page["next_cursor"].clone()).unwrap()),
            },
        )
        .await
        .unwrap();
    assert_eq!(next["events"].as_array().unwrap().len(), 1);
    assert_eq!(next["events"][0]["record_id"], "event-200");
    let mut stale: Cursor = serde_json::from_value(page["next_cursor"].clone()).unwrap();
    stale.data_revision -= 1;
    assert!(
        state
            .database
            .wellness_timeline(
                &user.user_id,
                TimelineRequest {
                    start_at: 0,
                    end_at: 200,
                    cursor: Some(stale)
                }
            )
            .await
            .is_err()
    );
}

pub(crate) fn tcx_fixture() -> String {
    r#"<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2"><Activities><Activity Sport="Biking"><Id>2026-01-01T00:00:00Z</Id><Lap StartTime="2026-01-01T00:00:00Z"><TotalTimeSeconds>60</TotalTimeSeconds><DistanceMeters>500</DistanceMeters><Calories>10</Calories><Intensity>Active</Intensity><TriggerMethod>Manual</TriggerMethod><Track><Trackpoint><Time>2026-01-01T00:00:00Z</Time><HeartRateBpm><Value>100</Value></HeartRateBpm><Cadence>80</Cadence></Trackpoint><Trackpoint><Time>2026-01-01T00:01:30Z</Time><HeartRateBpm><Value>110</Value></HeartRateBpm></Trackpoint></Track></Lap></Activity></Activities></TrainingCenterDatabase>"#.into()
}
#[tokio::test]
async fn tcx_keeps_indoor_points_source_timer_and_original_through_archive_lifecycle() {
    use helpyourself::wellness::import::{
        ImportDeleteRequest, ImportListRequest, XmlImportRequest, parse_tcx,
    };
    let xml = tcx_fixture();
    let record = parse_tcx(&XmlImportRequest {
        filename: "indoor.tcx".into(),
        xml: xml.clone(),
    })
    .unwrap();
    assert_eq!(record.payload["elapsed_seconds"], 90);
    assert_eq!(record.payload["source_total_time_seconds"], 60.0);
    assert_eq!(record.payload["distance_m"], 500.0);
    assert!(record.payload["laps"][0]["tracks"][0][0]["position"].is_null());
    assert_eq!(
        record.payload["laps"][0]["tracks"][0][0]["heart_rate_bpm"],
        100.0
    );
    assert_eq!(record.payload["original_utf8"], xml);
    assert!(
        parse_tcx(&XmlImportRequest {
            filename: "bad.tcx".into(),
            xml: xml.replace("<Value>100</Value>", "<Value>NaN</Value>")
        })
        .is_err()
    );
    assert!(
        parse_tcx(&XmlImportRequest {
            filename: "bad.tcx".into(),
            xml: xml.replace("00:01:30Z", "00:00:00Z")
        })
        .is_err()
    );
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "tcx-user").await;
    let imported = state
        .database
        .import_tcx(
            &user.user_id,
            XmlImportRequest {
                filename: "indoor.tcx".into(),
                xml,
            },
        )
        .await
        .unwrap();
    let listed = state
        .database
        .list_training_imports(&user.user_id, ImportListRequest { after_id: None })
        .await
        .unwrap();
    assert_eq!(listed["imports"][0]["source_id"], "tcx-2");
    let id = imported["record_id"].as_str().unwrap();
    state
        .database
        .delete_training_import(
            &user.user_id,
            ImportDeleteRequest {
                record_id: id.into(),
                source_id: "tcx-2".into(),
                expected_version: 1,
            },
        )
        .await
        .unwrap();
    assert!(
        state
            .database
            .list_training_imports(&user.user_id, ImportListRequest { after_id: None })
            .await
            .unwrap()["imports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn regularity_pipeline_requires_daytime_evidence_and_preserves_source_boundaries() {
    use helpyourself::{health::StoredHealthRecord, wellness::regularity::summarize_regularity};
    let row = |id: String, source: &str, start, end, category| StoredHealthRecord {
        platform: "apple_health".into(),
        source_id: source.into(),
        record_id: id,
        record_type: "sleep".into(),
        start_at: start,
        end_at: end,
        version: 1,
        payload_json: json!({"payload":{"category":category}}).to_string(),
    };
    let mut rows = Vec::new();
    for day in 0..7 {
        rows.push(row(
            format!("sleep-{day}"),
            "full",
            day * 86400,
            day * 86400 + 28800,
            1,
        ));
        rows.push(row(
            format!("wake-{day}"),
            "full",
            day * 86400 + 28800,
            (day + 1) * 86400,
            2,
        ));
        rows.push(row(
            format!("night-{day}"),
            "night-only",
            day * 86400,
            day * 86400 + 28800,
            1,
        ));
    }
    let result = summarize_regularity(rows, 0, 604800).unwrap();
    assert_eq!(result["sources"][0]["value"], 100.0);
    assert_eq!(result["sources"][0]["known_minutes"], 10080);
    assert!(result["sources"][1]["value"].is_null());
    assert_eq!(result["sources"][1]["state"], "insufficient_coverage");
    let conflict = summarize_regularity(
        vec![
            row("a".into(), "watch", 0, 604800, 1),
            row("b".into(), "watch", 0, 60, 2),
        ],
        0,
        604800,
    )
    .unwrap();
    assert_eq!(conflict["sources"][0]["state"], "conflicting_states");
    assert!(conflict["sources"][0]["value"].is_null());
    assert!(summarize_regularity(vec![], 0, 601200).is_err());
}

#[test]
fn training_windows_preserve_unknowns_and_require_complete_days() {
    use helpyourself::wellness::training::summarize;
    let end = 28 * 86400;
    let empty = summarize(&[], 0, end, chrono_tz::UTC).unwrap();
    assert!(empty["windows"][0]["total_load_au"].is_null());
    assert_eq!(empty["windows"][0]["complete_days"], 0);
    let mut entries:Vec<_> = (0..28).map(|day|json!({"record_id":format!("rest-{day}"),"version":1,"at":day*86400+3600,"timezone":"UTC","entry":{"kind":"training_day","content":{"status":"rest"}}})).collect();
    let rest = summarize(&entries, 0, end, chrono_tz::UTC).unwrap();
    assert_eq!(rest["windows"][1]["daily_mean_au"], 0.0);
    entries[27]["entry"]["content"]["status"] = json!("all_sessions_logged");
    entries.push(json!({"record_id":"training","version":1,"at":27*86400+7200,"timezone":"UTC","entry":{"kind":"training"},"calculation":{"session_load_au":300,"duration_basis":"elapsed_including_pauses"}}));
    let complete = summarize(&entries, 0, end, chrono_tz::UTC).unwrap();
    assert_eq!(complete["windows"][0]["total_load_au"], 300.0);
    assert_eq!(complete["windows"][1]["complete_days"], 28);
    entries[28]["calculation"]["session_load_au"] = json!(null);
    let missing = summarize(&entries, 0, end, chrono_tz::UTC).unwrap();
    assert_eq!(missing["days"][27]["state"], "missing_effort");
    assert!(missing["windows"][0]["daily_mean_au"].is_null());
    entries[27]["entry"]["content"]["status"] = json!("rest");
    let conflict = summarize(&entries, 0, end, chrono_tz::UTC).unwrap();
    assert_eq!(conflict["days"][27]["state"], "conflicting_declarations");
    entries[27]["timezone"] = json!("Europe/London");
    assert_eq!(
        summarize(&entries, 0, end, chrono_tz::UTC).unwrap()["days"][27]["state"],
        "unconfirmed"
    );
}

#[test]
fn training_windows_use_local_calendar_days_across_dst() {
    use chrono::TimeZone;
    let zone = chrono_tz::America::New_York;
    let end = zone
        .with_ymd_and_hms(2026, 3, 10, 12, 0, 0)
        .unwrap()
        .timestamp();
    let start = end - 30 * 86400;
    let at = zone
        .with_ymd_and_hms(2026, 3, 8, 23, 30, 0)
        .unwrap()
        .timestamp();
    let entry = json!({"record_id":"rest","version":1,"at":at,"timezone":"America/New_York","entry":{"kind":"training_day","content":{"status":"rest"}}});
    let result = helpyourself::wellness::training::summarize(&[entry], start, end, zone).unwrap();
    assert_eq!(result["days"][26]["date"], "2026-03-08");
    assert_eq!(result["days"][26]["state"], "confirmed_rest");
    assert_eq!(result["days"][27]["date"], "2026-03-09");
}

#[test]
fn heart_rate_zones_use_elapsed_time_and_fixed_boundary_rules() {
    use helpyourself::wellness::heart_rate::{DeclaredMaximum, summarize};
    let maximum = DeclaredMaximum {
        bpm: 200.0,
        source: "Synthetic measured maximum".into(),
    };
    let samples = [
        (0, 100.0),
        (60, 120.0),
        (120, 140.0),
        (180, 160.0),
        (240, 180.0),
        (300, 200.0),
        (360, 90.0),
    ];
    let result = summarize(&samples, 0, 600, 60, Some(&maximum)).unwrap();
    assert_eq!(result["zone_seconds"], json!([60, 60, 60, 60, 120]));
    assert_eq!(result["edwards_load_au"], 20.0);
    assert_eq!(result["covered_seconds"], 360);
    let gap = summarize(&samples, 0, 600, 59, Some(&maximum)).unwrap();
    assert_eq!(gap["covered_seconds"], 0);
    assert!(gap["edwards_load_au"].is_null());
    let absolute = summarize(&samples, 0, 600, 60, None).unwrap();
    assert_eq!(absolute["state"], "absolute_bins_only");
    assert!(absolute["zone_seconds"].is_null());
    let exceeded = summarize(&[(0, 201.0), (60, 180.0)], 0, 600, 60, Some(&maximum)).unwrap();
    assert_eq!(exceeded["state"], "maximum_exceeded");
    assert!(exceeded["edwards_load_au"].is_null());
    assert!(summarize(&[(0, 100.0), (0, 120.0)], 0, 600, 60, Some(&maximum)).is_err());
}

#[test]
fn sleep_corrections_preserve_observations_and_reject_stale_evidence() {
    use helpyourself::wellness::sleep::apply_corrections;
    let original = json!({"sources":[{"source":"apple_health:watch","sessions":[{"start_at":100,"end_at":1000,"asleep_seconds":600,"efficiency":0.8,"classification":"unclassified_session","timeline":[{"record_id":"sleep-1","version":1}]}]}]});
    let annotation = json!({"record_id":"correction","version":1,"entry":{"kind":"sleep_correction","content":{"source":"apple_health:watch","session_start":100,"session_end":1000,"classification":"nap","corrected_asleep_minutes":8.0,"basis_revisions":{"sleep-1":1},"note":"Synthetic correction"}}});
    let mut result = original.clone();
    apply_corrections(&mut result, std::slice::from_ref(&annotation)).unwrap();
    let session = &result["sources"][0]["sessions"][0];
    assert_eq!(session["asleep_seconds"], 600);
    assert_eq!(session["efficiency"], 0.8);
    assert_eq!(session["user_asleep_seconds"], 480.0);
    assert_eq!(session["classification"], "nap");
    let mut changed = original.clone();
    changed["sources"][0]["sessions"][0]["timeline"][0]["version"] = json!(2);
    apply_corrections(&mut changed, std::slice::from_ref(&annotation)).unwrap();
    assert_eq!(
        changed["sources"][0]["sessions"][0]["correction_state"],
        "stale_basis"
    );
    assert!(changed["sources"][0]["sessions"][0]["user_asleep_seconds"].is_null());
    let mut moved = original.clone();
    moved["sources"][0]["sessions"][0]["end_at"] = json!(1100);
    apply_corrections(&mut moved, std::slice::from_ref(&annotation)).unwrap();
    assert_eq!(
        moved["sources"][0]["sessions"][0]["correction_state"],
        "stale_basis"
    );
    let mut duplicate = original;
    apply_corrections(&mut duplicate, &[annotation.clone(), annotation]).unwrap();
    assert_eq!(
        duplicate["sources"][0]["sessions"][0]["correction_state"],
        "conflicting_corrections"
    );
}

#[tokio::test]
async fn sleep_correction_and_training_declaration_follow_manual_revision_lifecycle() {
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "alice").await;
    for (kind, content) in [
        (
            "training_day",
            json!({"status":"rest","note":"Synthetic rest day"}),
        ),
        (
            "sleep_correction",
            json!({"source":"apple_health:watch","session_start":100,"session_end":1000,"classification":"nap","corrected_asleep_minutes":8.0,"basis_revisions":{"sleep-1":1},"note":"Synthetic correction"}),
        ),
    ] {
        let id = uuid::Uuid::new_v4().to_string();
        let request = json!({"record_id":id,"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":{"kind":kind,"content":content}});
        state
            .database
            .save_wellness_entry(&user.user_id, serde_json::from_value(request).unwrap())
            .await
            .unwrap();
        let records = state
            .database
            .list_wellness_entries(
                &user.user_id,
                helpyourself::wellness::entries::ListRequest {
                    start_at: 0,
                    end_at: 2000,
                    kind: Some(kind.into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(records["entries"][0]["entry"]["content"], content);
        state
            .database
            .delete_wellness_entry(
                &user.user_id,
                helpyourself::wellness::entries::DeleteRequest {
                    record_id: id,
                    version: 2,
                    batch_id: uuid::Uuid::new_v4().to_string(),
                    kind: kind.into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            state
                .database
                .list_wellness_entries(
                    &user.user_id,
                    helpyourself::wellness::entries::ListRequest {
                        start_at: 0,
                        end_at: 2000,
                        kind: Some(kind.into())
                    }
                )
                .await
                .unwrap()["entries"],
            json!([])
        );
    }
}

#[test]
fn association_statistics_match_independent_reference_with_calendar_gaps() {
    use helpyourself::wellness::statistics::{Row, benjamini_yekutieli, regress};
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/statistics.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let rows: Vec<_> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| Row {
                day: r["day"].as_u64().unwrap() as usize,
                x: r["x"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect(),
                y: r["y"].as_f64().unwrap(),
            })
            .collect();
        let output = regress(&rows, 17).unwrap();
        for field in ["effect", "standard_error", "p_value"] {
            assert!(
                (output[field].as_f64().unwrap() - case["expected"][field].as_f64().unwrap()).abs()
                    < 1e-8,
                "{} {}",
                case["name"],
                field
            );
        }
        for i in 0..2 {
            assert!(
                (output["ci_95"][i].as_f64().unwrap()
                    - case["expected"]["ci_95"][i].as_f64().unwrap())
                .abs()
                    < 1e-8
            );
        }
        assert_eq!(output["bootstrap_successes"], 500);
        assert_eq!(output, regress(&rows, 17).unwrap());
    }
    let p: Vec<_> = fixture["by"]["p_values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64())
        .collect();
    for (actual, expected) in benjamini_yekutieli(&p)
        .iter()
        .zip(fixture["by"]["q_values"].as_array().unwrap())
    {
        if let Some(expected) = expected.as_f64() {
            assert!((actual.unwrap() - expected).abs() < 1e-12);
        } else {
            assert!(actual.is_none());
        }
    }
}

#[test]
fn association_journal_types_round_trip_and_invalid_values_are_rejected() {
    use helpyourself::wellness::entries::Entry;
    let valid = json!({"kind":"journal","content":{"behaviors":{"alcohol":false},"measurements":{"caffeine":{"value":120,"unit":"mg"}},"times":{"bedtime":1420},"mood":5,"perceived_stress":null,"note":"synthetic"}});
    let entry: Entry = serde_json::from_value(valid.clone()).unwrap();
    entry.validate().unwrap();
    let mut duplicate = valid.clone();
    duplicate["content"]["times"]["alcohol"] = json!(100);
    assert!(
        serde_json::from_value::<Entry>(duplicate)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut bad = valid;
    bad["content"]["times"]["bedtime"] = json!(1440);
    assert!(
        serde_json::from_value::<Entry>(bad)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[tokio::test]
async fn associations_preserve_missingness_isolate_owners_and_invalidate_on_source_edit() {
    use helpyourself::wellness::associations::Request;
    let (_directory, state) = crate::fixture().await;
    let alice = crate::add_user(&state, "association-alice").await;
    let bob = crate::add_user(&state, "association-bob").await;
    let result = state
        .database
        .behavior_associations(
            &alice.user_id,
            Request {
                end_date: "2026-01-01".into(),
                timezone: "UTC".into(),
                outcome: "sleep".into(),
                source: "apple_health:watch".into(),
                behaviors: vec!["caffeine".into(), "alcohol".into()],
                covariates: vec![],
                lag_days: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(result["results"].as_array().unwrap().len(), 2);
    assert_eq!(result["results"][0]["state"], "insufficient_days");
    assert!(result["results"][0]["q_value"].is_null());
    assert!(result["results"][0]["passes_by_threshold"].is_null());
    assert_eq!(
        result["calibration"]["significance_decisions_enabled"],
        false
    );
    let id = result["result_id"].as_str().unwrap();
    assert!(
        state
            .database
            .association_result(&bob.user_id, id)
            .await
            .is_err()
    );
    assert_eq!(
        state
            .database
            .association_result(&alice.user_id, id)
            .await
            .unwrap(),
        result
    );
    state.database.save_wellness_entry(&alice.user_id,serde_json::from_value(json!({"record_id":uuid::Uuid::new_v4(),"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":{"kind":"journal","content":{"behaviors":{"alcohol":false},"measurements":{},"times":{},"mood":null,"perceived_stress":null,"note":"synthetic"}}})).unwrap()).await.unwrap();
    assert!(
        state
            .database
            .association_result(&alice.user_id, id)
            .await
            .is_err()
    );
    assert_eq!(
        state
            .database
            .association_results(&alice.user_id)
            .await
            .unwrap()["results"],
        json!([])
    );
}

#[test]
fn association_daily_pipeline_handles_types_missing_days_and_clock_boundaries() {
    use chrono::{Days, NaiveDate};
    use helpyourself::wellness::associations::{Request, analyze};
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/statistics.json")).unwrap();
    let start = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
    let mut request = Request {
        end_date: "2026-04-04".into(),
        timezone: "UTC".into(),
        outcome: "hrv_sdnn".into(),
        source: "apple_health:watch".into(),
        behaviors: vec!["synthetic".into(), "never_recorded".into()],
        covariates: vec![],
        lag_days: 0,
    };
    let mut journals = Vec::new();
    let mut outcomes = Vec::new();
    for row in fixture["cases"][0]["rows"].as_array().unwrap() {
        let date = start
            .checked_add_days(Days::new(row["day"].as_u64().unwrap()))
            .unwrap();
        journals.push(json!({"at":date.and_hms_opt(12,0,0).unwrap().and_utc().timestamp(),"timezone":"UTC","entry":{"content":{"behaviors":{"synthetic":row["x"][1].as_f64()==Some(1.0)},"measurements":{},"times":{}}}}));
        outcomes.push(json!({"date":date.to_string(),"source":"apple_health:watch","value":row["y"],"unit":"ms","excluded_sample_count":0}));
    }
    let result = analyze(&request, start, &json!(journals), &json!(outcomes)).unwrap();
    assert_eq!(result["results"][0]["state"], "estimated");
    assert!(
        (result["results"][0]["estimate"]["effect"].as_f64().unwrap()
            - fixture["cases"][0]["expected"]["effect"].as_f64().unwrap())
        .abs()
            < 1e-8
    );
    assert_eq!(result["results"][1]["state"], "insufficient_days");
    journals[0]["entry"]["content"]["behaviors"] = json!({});
    journals[0]["entry"]["content"]["measurements"] = json!({"synthetic":{"value":1,"unit":"mg"}});
    assert_eq!(
        analyze(&request, start, &json!(journals), &json!(outcomes)).unwrap()["results"][0]["state"],
        "behavior_type_or_unit_changed"
    );
    request.behaviors = vec!["bedtime".into()];
    for (index, journal) in journals.iter_mut().enumerate() {
        journal["entry"]["content"]["times"] = json!({"bedtime":if index%2==0{719}else{721}});
    }
    assert_eq!(
        analyze(&request, start, &json!(journals), &json!(outcomes)).unwrap()["results"][0]["state"],
        "clock_time_crosses_anchor"
    );
}

#[tokio::test]
async fn training_protocol_rejects_inconsistent_times_and_preserves_answer_delay() {
    use helpyourself::wellness::entries::{Entry, SaveRequest};
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "training-protocol").await;
    let entry = json!({"kind":"training","content":{"activity":"Run","duration_minutes":40,"ended_at":2800,"paused_minutes":5,"duration_basis":"active_excluding_pauses","rpe_answered_at":4600,"rpe_cr10":6,"sets":[],"note":""}});
    let typed: Entry = serde_json::from_value(entry.clone()).unwrap();
    let calculation = typed.calculation().unwrap();
    assert_eq!(calculation["session_load_au"], 240.0);
    assert_eq!(calculation["rpe_delay_minutes"], 30.0);
    for (key, value) in [
        ("rpe_answered_at", json!(null)),
        ("rpe_answered_at", json!(2700)),
        ("duration_basis", json!("apple_effort")),
        ("paused_minutes", json!(-1)),
    ] {
        let mut bad = entry.clone();
        bad["content"][key] = value;
        assert!(
            serde_json::from_value::<Entry>(bad)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    for (start, expected) in [(100, true), (101, true), (102, false), (2800, false)] {
        let request:SaveRequest=serde_json::from_value(json!({"record_id":uuid::Uuid::new_v4(),"version":1,"batch_id":uuid::Uuid::new_v4(),"at":start,"timezone":"UTC","entry":entry})).unwrap();
        assert_eq!(
            state
                .database
                .save_wellness_entry(&user.user_id, request)
                .await
                .is_ok(),
            expected
        );
    }
}

fn nn_payload() -> serde_json::Value {
    let mut offset = 0.0;
    let intervals: Vec<_> = (0..300)
        .map(|i| {
            let nn = if i % 2 == 0 { 990.0 } else { 1010.0 };
            offset += nn / 1000.0;
            json!({"offset_seconds":offset,"nn_ms":nn,"quality":"normal","preceded_by_gap":false})
        })
        .collect();
    json!({"schema":"nn-intervals-v1","protocol_id":"rest-5min","context":"morning_rest","device_model":"fixture","firmware":"1","posture":"supine","quality_reviewed":true,"intervals":intervals})
}
#[test]
fn hrv_requires_five_continuous_reviewed_minutes_and_separates_protocol_baselines() {
    use helpyourself::wellness::hrv::{Intervals, calculate, summarize};
    let payload = nn_payload();
    let result = calculate(&serde_json::from_value::<Intervals>(payload.clone()).unwrap()).unwrap();
    assert!((result["rmssd_ms"].as_f64().unwrap() - 20.0).abs() < 1e-10);
    assert!((result["ln_rmssd"].as_f64().unwrap() - 20f64.ln()).abs() < 1e-10);
    for mutation in ["gap", "quality", "offset", "duration", "review"] {
        let mut bad = payload.clone();
        match mutation {
            "gap" => bad["intervals"][120]["preceded_by_gap"] = json!(true),
            "quality" => bad["intervals"][120]["quality"] = json!("unknown"),
            "offset" => bad["intervals"][120]["offset_seconds"] = json!(1),
            "duration" => {
                bad["intervals"].as_array_mut().unwrap().pop();
            }
            _ => bad["quality_reviewed"] = json!(false),
        }
        assert!(calculate(&serde_json::from_value::<Intervals>(bad).unwrap()).is_err());
    }
    let rows = (0..30)
        .map(|day| {
            let mut p = payload.clone();
            if day == 29 {
                p["firmware"] = json!("2");
            }
            helpyourself::health::StoredHealthRecord {
                platform: "manual".into(),
                source_id: "test-device".into(),
                record_id: format!("nn-{day}"),
                record_type: "nn_intervals".into(),
                start_at: day * 86400,
                end_at: day * 86400 + 300,
                version: 1,
                payload_json: json!({"payload":p}).to_string(),
            }
        })
        .collect();
    let result = summarize(rows, chrono_tz::UTC).unwrap();
    assert_eq!(result.as_array().unwrap().len(), 2);
    let prior = result
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["protocol"]["firmware"] == "1")
        .unwrap();
    assert!(prior["days"][27]["baseline"].is_null());
    assert_eq!(prior["days"][28]["baseline_days"], 28);
    assert!(prior["days"][28]["baseline"]["z"].is_null());
    let changed = result
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["protocol"]["firmware"] == "2")
        .unwrap();
    assert_eq!(changed["days"][0]["baseline_days"], 0);
}

#[tokio::test]
async fn exercise_library_is_owned_versioned_and_plans_never_become_completed_load() {
    use helpyourself::wellness::{
        entries::{DeleteRequest, Entry, ListRequest, SaveRequest},
        planning::LibraryRequest,
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "planner").await;
    let other = crate::add_user(&state, "planner-other").await;
    let id = uuid::Uuid::new_v4().to_string();
    let exercise = json!({"kind":"exercise","content":{"name":"Squat","equipment":"Barbell","muscle_groups":["quadriceps"],"instructions":"Use your agreed training technique.","note":""}});
    let save = |version, entry| {
        serde_json::from_value::<SaveRequest>(json!({"record_id":id,"version":version,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":entry})).unwrap()
    };
    state
        .database
        .save_wellness_entry(&user.user_id, save(1, exercise.clone()))
        .await
        .unwrap();
    let library = state
        .database
        .wellness_library(
            &user.user_id,
            LibraryRequest {
                kind: "exercise".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(library["entries"][0]["entry"], exercise);
    assert!(
        state
            .database
            .wellness_library(
                &other.user_id,
                LibraryRequest {
                    kind: "exercise".into()
                }
            )
            .await
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let block = json!({"exercise":"Squat","equipment":"Barbell","sets":3,"repetitions":5,"duration_seconds":null,"external_weight_kg":60,"rest_seconds":120});
    let plan = json!({"kind":"planned_workout","content":{"title":"Monday","duration_minutes":45,"status":"planned","blocks":[block],"note":""}});
    let typed: Entry = serde_json::from_value(plan.clone()).unwrap();
    assert!(typed.calculation().unwrap()["session_load_au"].is_null());
    let request:SaveRequest=serde_json::from_value(json!({"record_id":uuid::Uuid::new_v4(),"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":plan})).unwrap();
    state
        .database
        .save_wellness_entry(&user.user_id, request)
        .await
        .unwrap();
    let list = state
        .database
        .list_wellness_entries(
            &user.user_id,
            ListRequest {
                start_at: 0,
                end_at: 86400,
                kind: None,
            },
        )
        .await
        .unwrap();
    let training = helpyourself::wellness::training::summarize(
        list["entries"].as_array().unwrap(),
        0,
        86400,
        chrono_tz::UTC,
    )
    .unwrap();
    assert!(
        training["days"]
            .as_array()
            .unwrap()
            .iter()
            .all(|day| day["sessions"] == 0 && day["observed_sum_au"].is_null())
    );
    let mut invalid = plan.clone();
    invalid["content"]["status"] = json!("completed");
    assert!(
        serde_json::from_value::<Entry>(invalid)
            .unwrap()
            .validate()
            .is_err()
    );
    state
        .database
        .delete_wellness_entry(
            &user.user_id,
            DeleteRequest {
                record_id: id,
                version: 2,
                batch_id: uuid::Uuid::new_v4().to_string(),
                kind: "exercise".into(),
            },
        )
        .await
        .unwrap();
    assert!(
        state
            .database
            .wellness_library(
                &user.user_id,
                LibraryRequest {
                    kind: "exercise".into()
                }
            )
            .await
            .unwrap()["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn food_portions_preserve_unknowns_snapshots_versions_and_user_isolation() {
    use helpyourself::wellness::{
        entries::SaveRequest,
        food::{PortionBasis, PortionRequest, Recipe, recipe_totals},
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "food-owner").await;
    let other = crate::add_user(&state, "food-other").await;
    let recipe = json!({"name":"Two foods","servings":2,"instructions":"Mix","note":"","ingredients":[{"food_name":"A","grams":200,"source":"Label A","nutrients_per_100g":{"energy_kcal":100,"sodium_mg":10}},{"food_name":"B","grams":100,"source":"Label B","nutrients_per_100g":{"energy_kcal":200}}]});
    let parsed: Recipe = serde_json::from_value(recipe.clone()).unwrap();
    let totals = recipe_totals(&parsed).unwrap();
    assert_eq!(totals["nutrients"]["energy_kcal"]["per_serving"], 200.0);
    assert_eq!(totals["nutrients"]["sodium_mg"]["observed_total"], 20.0);
    assert!(totals["nutrients"]["sodium_mg"]["per_serving"].is_null());
    let id = uuid::Uuid::new_v4().to_string();
    let entry = json!({"kind":"recipe","content":recipe});
    let save:SaveRequest=serde_json::from_value(json!({"record_id":id,"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":entry})).unwrap();
    state
        .database
        .save_wellness_entry(&user.user_id, save)
        .await
        .unwrap();
    let request = |version, unit: &str| PortionRequest {
        basis: PortionBasis {
            record_id: id.clone(),
            version,
            amount: 1.5,
            unit: unit.into(),
        },
        meal: "lunch".into(),
    };
    let portion = state
        .database
        .food_portion(&user.user_id, request(1, "servings"))
        .await
        .unwrap();
    assert_eq!(portion["nutrients"]["energy_kcal"], 300.0);
    assert!(portion["nutrients"]["sodium_mg"].is_null());
    assert_eq!(portion["incomplete_nutrients"], json!(["sodium_mg"]));
    assert_eq!(
        portion["source_snapshot"],
        serde_json::to_value(
            serde_json::from_value::<helpyourself::wellness::entries::Entry>(entry).unwrap()
        )
        .unwrap()
    );
    assert!(
        state
            .database
            .food_portion(&other.user_id, request(1, "servings"))
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .food_portion(&user.user_id, request(2, "servings"))
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .food_portion(&user.user_id, request(1, "g"))
            .await
            .is_err()
    );
    let mut invalid = recipe;
    invalid["ingredients"][0]["nutrients_per_100g"] = json!({"sodium_g":1});
    assert!(!serde_json::from_value::<Recipe>(invalid).unwrap().valid());
}

#[test]
fn nutrition_targets_preserve_zero_and_missing_values() {
    use helpyourself::wellness::food::daily_nutrients;
    let goal = json!({"entry":{"content":{"daily_targets":{"sodium_mg":1000,"protein_g":100}}}});
    let records = vec![
        json!({"entry":{"content":{"protein_g":20,"micronutrients":{"sodium_mg":0}}}}),
        json!({"entry":{"content":{"protein_g":10,"micronutrients":{}}}}),
    ];
    let result = daily_nutrients(&records, Some(&goal));
    let sodium = result
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["nutrient"] == "sodium_mg")
        .unwrap();
    assert_eq!(sodium["observed_sum"], 0.0);
    assert_eq!(sodium["missing_records"], 1);
    assert!(sodium["recorded_fraction_of_target"].is_null());
    let protein = result
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["nutrient"] == "protein_g")
        .unwrap();
    assert_eq!(protein["recorded_fraction_of_target"], 0.3);
    assert!(
        daily_nutrients(&[], Some(&goal))
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["observed_sum"].is_null())
    );
}

#[tokio::test]
async fn hrv_archive_query_is_owned_and_keeps_unknown_apple_quality_explicit() {
    use helpyourself::{
        health::{ConnectionRequest, HealthRecordInput, SyncRequest},
        wellness::hrv::Request,
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "hrv-owner").await;
    let other = crate::add_user(&state, "hrv-other").await;
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    for (kind, payload) in [
        ("nn_intervals", nn_payload()),
        (
            "heartbeat_series",
            json!({"samples":[{"time_since_start":1,"preceded_by_gap":false}]}),
        ),
    ] {
        state
            .database
            .sync_health(
                &user.user_id,
                SyncRequest {
                    connection_id: connection.connection_id.clone(),
                    batch_id: uuid::Uuid::new_v4().to_string(),
                    record_type: kind.into(),
                    coverage_status: "observed".into(),
                    records: vec![HealthRecordInput {
                        record_id: kind.into(),
                        source_id: "test".into(),
                        record_type: kind.into(),
                        start_at: 100,
                        end_at: 400,
                        version: 1,
                        deleted: false,
                        payload,
                    }],
                },
            )
            .await
            .unwrap();
    }
    let request = || Request {
        start_at: 0,
        end_at: 1000,
        timezone: "UTC".into(),
    };
    let result = state
        .database
        .hrv_windows(&user.user_id, request())
        .await
        .unwrap();
    assert_eq!(result["sources"].as_array().unwrap().len(), 2);
    assert!(
        result["sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["windows"][0]["state"] == "quality_or_protocol_unknown")
    );
    assert!(
        state
            .database
            .hrv_windows(&other.user_id, request())
            .await
            .unwrap()["sources"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn specialty_views_page_samples_decode_fhir_and_never_resolve_another_source() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use helpyourself::{
        health::{ConnectionRequest, HealthRecordInput, SyncRequest},
        wellness::records::{GetRequest, ListRequest},
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "specialty").await;
    let other = crate::add_user(&state, "specialty-other").await;
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    let resource = json!({"resourceType":"MedicationStatement","status":"active","medicationCodeableConcept":{"text":"Synthetic medication"},"text":{"div":"<script>not executable</script>"}});
    for (kind, id, source, payload) in [
        (
            "electrocardiogram",
            "ecg",
            "watch",
            json!({"ecg_classification":1,"series":(0..5001).map(|i|json!({"time_since_start":i as f64/512.0,"lead_i_volts":0.001})).collect::<Vec<_>>()}),
        ),
        (
            "clinical",
            "clinical",
            "watch",
            json!({"fhir":{"data_base64":STANDARD.encode(resource.to_string())}}),
        ),
        (
            "HKCorrelationTypeIdentifierBloodPressure",
            "bp",
            "watch",
            json!({"related_sample_ids":["systolic","diastolic"]}),
        ),
        (
            "systolic",
            "systolic",
            "watch",
            json!({"value":120,"unit":"mmHg"}),
        ),
        (
            "diastolic",
            "diastolic",
            "other-device",
            json!({"value":80,"unit":"mmHg"}),
        ),
    ] {
        state
            .database
            .sync_health(
                &user.user_id,
                SyncRequest {
                    connection_id: connection.connection_id.clone(),
                    batch_id: uuid::Uuid::new_v4().to_string(),
                    record_type: kind.into(),
                    coverage_status: "observed".into(),
                    records: vec![HealthRecordInput {
                        record_id: id.into(),
                        source_id: source.into(),
                        record_type: kind.into(),
                        start_at: 100,
                        end_at: 110,
                        version: 1,
                        deleted: false,
                        payload,
                    }],
                },
            )
            .await
            .unwrap();
    }
    let request = |id: &str, offset| GetRequest {
        platform: "apple_health".into(),
        source_id: "watch".into(),
        record_id: id.into(),
        sample_offset: offset,
    };
    let ecg = state
        .database
        .specialty_record(&user.user_id, request("ecg", 0))
        .await
        .unwrap();
    assert_eq!(ecg["samples"].as_array().unwrap().len(), 5000);
    assert_eq!(ecg["next_sample_offset"], 5000);
    assert_eq!(ecg["source_classification"], 1);
    assert_eq!(
        state
            .database
            .specialty_record(&user.user_id, request("ecg", 5000))
            .await
            .unwrap()["samples"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let clinical = state
        .database
        .specialty_record(&user.user_id, request("clinical", 0))
        .await
        .unwrap();
    assert_eq!(clinical["fhir"]["resource"], resource);
    let bp = state
        .database
        .specialty_record(&user.user_id, request("bp", 0))
        .await
        .unwrap();
    assert_eq!(bp["related"][0]["records"][0]["value"], 120.0);
    assert_eq!(bp["related"][1]["state"], "missing");
    assert!(
        state
            .database
            .specialty_record(&other.user_id, request("ecg", 0))
            .await
            .is_err()
    );
    for kind in ["ecg", "clinical", "blood_pressure"] {
        let list = state
            .database
            .specialty_records(
                &user.user_id,
                ListRequest {
                    kind: kind.into(),
                    after: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(list["records"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn reminders_obey_calendar_dst_quiet_hours_and_disable_state() {
    use chrono::TimeZone;
    use helpyourself::wellness::reminders::{Reminder, occurrences};
    let record = json!({"record_id":"reminder","version":1,"timezone":"America/New_York","entry":{"kind":"reminder","content":{"title":"Follow-up","body":"Review your plan","enabled":true,"recurrence":"daily","start_date":"2026-03-07","end_date":null,"local_time":"02:30","weekdays":[],"quiet_start_minute":null,"quiet_end_minute":null,"note":""}}});
    let from = chrono_tz::America::New_York
        .with_ymd_and_hms(2026, 3, 7, 0, 0, 0)
        .unwrap()
        .timestamp();
    let result = occurrences(&record, from, from + 3 * 86400).unwrap();
    assert_eq!(
        result.iter().find(|v| v["date"] == "2026-03-08").unwrap()["state"],
        "nonexistent_local_time"
    );
    assert_eq!(
        result.iter().filter(|v| v["state"] == "scheduled").count(),
        2
    );
    let mut quiet = record.clone();
    quiet["entry"]["content"]["quiet_start_minute"] = json!(1380);
    quiet["entry"]["content"]["quiet_end_minute"] = json!(420);
    assert!(
        occurrences(&quiet, from, from + 86400)
            .unwrap()
            .iter()
            .all(|v| v["state"] == "quiet_time")
    );
    quiet["entry"]["content"]["enabled"] = json!(false);
    assert!(occurrences(&quiet, from, from + 86400).unwrap().is_empty());
    let mut weekly = record.clone();
    weekly["entry"]["content"]["recurrence"] = json!("weekly");
    weekly["entry"]["content"]["weekdays"] = json!([6]);
    assert_eq!(
        occurrences(&weekly, from, from + 7 * 86400).unwrap().len(),
        1
    );
    let mut invalid = record["entry"]["content"].clone();
    invalid["quiet_start_minute"] = json!(10);
    assert!(!serde_json::from_value::<Reminder>(invalid).unwrap().valid());
    let mut fall = record.clone();
    fall["entry"]["content"]["start_date"] = json!("2026-11-01");
    fall["entry"]["content"]["recurrence"] = json!("once");
    fall["entry"]["content"]["local_time"] = json!("01:30");
    let from = chrono_tz::America::New_York
        .with_ymd_and_hms(2026, 11, 1, 0, 0, 0)
        .unwrap()
        .timestamp();
    let result = occurrences(&fall, from, from + 25 * 3600).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["at"], from + 90 * 60);
}

#[tokio::test]
async fn daily_materialization_invalidates_recomputes_and_bounds_failed_leases() {
    use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "daily-cache").await;
    let request = || DayRequest {
        date: "2026-10-05".into(),
        timezone: "UTC".into(),
        source_priority: BTreeMap::new(),
    };
    let first = state
        .database
        .wellness_day(&user.user_id, request())
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .wellness_day(&user.user_id, request())
            .await
            .unwrap(),
        first
    );
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(state.config.data_dir.join("database.sqlite"))
            .foreign_keys(true),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE users SET data_revision=data_revision+1 WHERE user_id=?")
        .bind(&user.user_id)
        .execute(&mut db)
        .await
        .unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM daily_views WHERE user_id=?")
        .bind(&user.user_id)
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert_eq!(status, "queued");
    let result: Option<String> =
        sqlx::query_scalar("SELECT result_json FROM daily_views WHERE user_id=?")
            .bind(&user.user_id)
            .fetch_one(&mut db)
            .await
            .unwrap();
    assert!(result.is_none());
    let (a, b) = tokio::join!(
        state.database.recompute_daily_next(1000, 60),
        state.database.recompute_daily_next(1000, 60)
    );
    assert_ne!(a.unwrap(), b.unwrap());
    let next = state
        .database
        .wellness_day(&user.user_id, request())
        .await
        .unwrap();
    assert!(next["data_revision"].as_i64().unwrap() > first["data_revision"].as_i64().unwrap());
    sqlx::query("UPDATE daily_views SET status='running',lease_until=0,claim_token='expired',attempts=1 WHERE user_id=?").bind(&user.user_id).execute(&mut db).await.unwrap();
    assert!(state.database.recompute_daily_next(1000, 60).await.unwrap());
    sqlx::query(
        "UPDATE daily_views SET status='queued',parameters_json='{}',attempts=0 WHERE user_id=?",
    )
    .bind(&user.user_id)
    .execute(&mut db)
    .await
    .unwrap();
    for _ in 0..3 {
        assert!(state.database.recompute_daily_next(1000, 60).await.unwrap());
    }
    assert!(!state.database.recompute_daily_next(1000, 60).await.unwrap());
    let status: String = sqlx::query_scalar("SELECT status FROM daily_views WHERE user_id=?")
        .bind(&user.user_id)
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert_eq!(status, "failed");
}

#[test]
fn coach_rejects_invented_citations_and_unrequested_or_unsupported_drafts() {
    use helpyourself::wellness::coach::{Output, validate};
    let input = json!({"evidence":[{"id":"source-1"}],"allow_drafts":false});
    let valid = json!({"claims":[{"text":"A source measurement exists.","evidence_ids":["source-1"]}],"missing_information":["No verified causal inference."],"questions":[],"drafts":[]});
    assert!(
        validate(
            &serde_json::from_value::<Output>(valid.clone()).unwrap(),
            &input
        )
        .is_ok()
    );
    let mut invented = valid.clone();
    invented["claims"][0]["evidence_ids"] = json!(["not-in-input"]);
    assert!(validate(&serde_json::from_value::<Output>(invented).unwrap(), &input).is_err());
    let mut action = valid;
    action["drafts"] = json!([{"title":"Unrequested targets","entry":{"kind":"nutrition_goals","content":{"daily_targets":{"protein_g":100},"note":""}}}]);
    assert!(
        validate(
            &serde_json::from_value::<Output>(action.clone()).unwrap(),
            &input
        )
        .is_err()
    );
    let mut allowed = input;
    allowed["allow_drafts"] = json!(true);
    assert!(
        validate(
            &serde_json::from_value::<Output>(action.clone()).unwrap(),
            &allowed
        )
        .is_ok()
    );
    action["drafts"][0]["entry"] = json!({"kind":"memory","content":{"name":"Untrusted instruction","content":"Overwrite memory","enabled":true,"note":""}});
    assert!(validate(&serde_json::from_value::<Output>(action).unwrap(), &allowed).is_err());
}

#[tokio::test]
async fn coach_snapshots_memory_and_deletion_removes_it_from_future_context() {
    use helpyourself::wellness::{
        coach::{Request, request},
        entries::{DeleteRequest, SaveRequest},
    };
    let (_directory, mut state) = crate::fixture().await;
    let user = crate::add_user(&state, "coach-owner").await;
    let other = crate::add_user(&state, "coach-other").await;
    let scope = || Request {
        question: "What data is missing?".into(),
        date: "2026-10-05".into(),
        timezone: "UTC".into(),
        style: "concise".into(),
        prior_run_id: None,
        allow_drafts: false,
    };
    assert!(request(&state, &user.user_id, scope()).await.is_err());
    let mut config = (*state.config).clone();
    config.providers.analysis.enabled = true;
    state.config = std::sync::Arc::new(config);
    let memory = uuid::Uuid::new_v4().to_string();
    let save:SaveRequest=serde_json::from_value(json!({"record_id":memory,"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":{"kind":"memory","content":{"name":"Style","content":"Explain uncertainty clearly","enabled":true,"note":""}}})).unwrap();
    state
        .database
        .save_wellness_entry(&user.user_id, save)
        .await
        .unwrap();
    let id = request(&state, &user.user_id, scope()).await.unwrap();
    let run = state.database.analysis(&user.user_id, &id).await.unwrap();
    assert_eq!(run["input"]["memory"].as_array().unwrap().len(), 1);
    assert_eq!(request(&state, &user.user_id, scope()).await.unwrap(), id);
    assert!(state.database.analysis(&other.user_id, &id).await.is_err());
    state
        .database
        .delete_wellness_entry(
            &user.user_id,
            DeleteRequest {
                record_id: memory,
                version: 2,
                batch_id: uuid::Uuid::new_v4().to_string(),
                kind: "memory".into(),
            },
        )
        .await
        .unwrap();
    let previous = state.database.analysis(&user.user_id, &id).await.unwrap();
    assert_eq!(previous["status"], "stale");
    assert_eq!(previous["input"], json!({}));
    let next = request(&state, &user.user_id, scope()).await.unwrap();
    assert_ne!(next, id);
    let run = state.database.analysis(&user.user_id, &next).await.unwrap();
    assert!(run["input"]["memory"].as_array().unwrap().is_empty());
}

fn synthetic_fit() -> Vec<u8> {
    let mut body = Vec::new();
    let definition = |body: &mut Vec<u8>, local: u8, global: u16, fields: &[(u8, u8, u8)]| {
        body.extend([0x40 | local, 0, 0]);
        body.extend(global.to_le_bytes());
        body.push(fields.len() as u8);
        for &(id, size, kind) in fields {
            body.extend([id, size, kind]);
        }
    };
    definition(&mut body, 0, 0, &[(0, 1, 0)]);
    body.extend([0, 4]);
    definition(
        &mut body,
        1,
        18,
        &[
            (253, 4, 0x86),
            (2, 4, 0x86),
            (7, 4, 0x86),
            (8, 4, 0x86),
            (9, 4, 0x86),
        ],
    );
    body.push(1);
    for value in [1_000_000_300u32, 1_000_000_000, 300_000, 290_000, 100_000] {
        body.extend(value.to_le_bytes());
    }
    definition(
        &mut body,
        2,
        20,
        &[(253, 4, 0x86), (7, 2, 0x84), (4, 1, 2), (3, 1, 2)],
    );
    body.push(2);
    body.extend(1_000_000_100u32.to_le_bytes());
    body.extend(200u16.to_le_bytes());
    body.extend([85, 140]);
    let mut bytes = vec![12, 0x20, 0, 0];
    bytes.extend((body.len() as u32).to_le_bytes());
    bytes.extend(b".FIT");
    bytes.extend(body);
    let mut crc = 0u16;
    for byte in &bytes {
        crc ^= *byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    bytes.extend(crc.to_le_bytes());
    bytes
}

#[test]
fn fit_preserves_originals_and_source_units_and_rejects_corruption() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use helpyourself::wellness::fit::{Request, parse, preflight};
    let bytes = synthetic_fit();
    let request = |bytes: &[u8]| Request {
        filename: "synthetic.fit".into(),
        data_base64: STANDARD.encode(bytes),
    };
    let record = parse(&request(&bytes)).unwrap();
    assert_eq!(record.end_at - record.start_at, 300);
    assert_eq!(record.payload["source_total_time_seconds"], 290.0);
    assert_eq!(record.payload["distance_m"], 1000.0);
    assert_eq!(record.payload["points"][0]["power_watts"], 200.0);
    assert_eq!(record.payload["points"][0]["cadence_rpm"], 85.0);
    assert_eq!(record.payload["original_base64"], STANDARD.encode(&bytes));
    assert_eq!(
        record.payload["decoded_messages"].as_array().unwrap().len(),
        3
    );
    let mut broken = bytes.clone();
    broken[20] ^= 1;
    assert!(parse(&request(&broken)).is_err());
    assert!(parse(&request(&bytes[..bytes.len() - 1])).is_err());
    let mut huge = vec![12, 0x20, 0, 0];
    huge.extend(50007u32.to_le_bytes());
    huge.extend(b".FIT");
    huge.extend([0x40, 0, 0, 0, 0, 0]);
    huge.extend(vec![0; 50001]);
    huge.extend([0, 0]);
    assert!(preflight(&huge).is_err());
}

#[tokio::test]
async fn fit_import_deduplicates_and_restricts_deletion_to_owner() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use helpyourself::wellness::{
        fit::Request,
        import::{ImportDeleteRequest, ImportListRequest},
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "fit-owner").await;
    let other = crate::add_user(&state, "fit-other").await;
    let request = || Request {
        filename: "synthetic.fit".into(),
        data_base64: STANDARD.encode(synthetic_fit()),
    };
    let first = state
        .database
        .import_fit(&user.user_id, request())
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .import_fit(&user.user_id, request())
            .await
            .unwrap()["duplicate"],
        true
    );
    assert!(
        state
            .database
            .list_training_imports(&other.user_id, ImportListRequest { after_id: None })
            .await
            .unwrap()["imports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let detail = state
        .database
        .specialty_record(
            &user.user_id,
            helpyourself::wellness::records::GetRequest {
                platform: "file_import".into(),
                source_id: "fit".into(),
                record_id: first["record_id"].as_str().unwrap().into(),
                sample_offset: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(detail["samples"][0]["power_watts"], 200.0);
    assert_eq!(detail["source_total_time_seconds"], 290.0);
    assert_eq!(detail["elapsed_seconds"], 300);
    let deletion = || ImportDeleteRequest {
        source_id: "fit".into(),
        record_id: first["record_id"].as_str().unwrap().into(),
        expected_version: 1,
    };
    assert!(
        state
            .database
            .delete_training_import(&other.user_id, deletion())
            .await
            .is_err()
    );
    state
        .database
        .delete_training_import(&user.user_id, deletion())
        .await
        .unwrap();
    assert!(
        state
            .database
            .list_training_imports(&user.user_id, ImportListRequest { after_id: None })
            .await
            .unwrap()["imports"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn calendar_report_keeps_sources_and_missing_days_separate() {
    use helpyourself::wellness::report::summarize_days;
    let rows = vec![
        json!({"source":"a","date":"2026-01-01","value":10.0}),
        json!({"source":"a","date":"2026-01-02","value":30.0}),
        json!({"source":"b","date":"2026-01-01","value":100.0}),
    ];
    let result = summarize_days(&rows, 3).unwrap();
    assert_eq!(result[0]["daily_median"], 20.0);
    assert_eq!(result[0]["missing_days"], 1);
    assert_eq!(result[1]["daily_median"], 100.0);
    assert_eq!(result[1]["missing_days"], 2);
    assert!(summarize_days(&[rows[0].clone(), rows[0].clone()], 3).is_err());
}
#[tokio::test]
async fn calendar_report_validates_bounds_and_preserves_empty_state() {
    use helpyourself::wellness::report::Request;
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "calendar-report").await;
    let request = |end: &str| Request {
        start_date: "2026-01-01".into(),
        end_date: end.into(),
        timezone: "America/New_York".into(),
    };
    let result = state
        .database
        .wellness_report(&user.user_id, request("2026-12-31"))
        .await
        .unwrap();
    assert_eq!(result["calendar_days"], 365);
    assert_eq!(result["metrics"].as_array().unwrap().len(), 6);
    assert!(
        result["metrics"][0]["sources"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .database
            .wellness_report(&user.user_id, request("2027-02-01"))
            .await
            .is_err()
    );
}

fn complete_diet_totals() -> BTreeMap<String, f64> {
    serde_json::from_value(json!({"energy_kcal":1000,"total_fruit_cup_eq":0.8,"whole_fruit_cup_eq":0.4,"vegetables_cup_eq":1.1,"greens_beans_cup_eq":0.2,"whole_grains_oz_eq":1.5,"dairy_cup_eq":1.3,"protein_foods_oz_eq":2.5,"seafood_plant_oz_eq":0.8,"unsaturated_fat_g":20,"saturated_fat_g":8,"refined_grains_oz_eq":1.8,"sodium_mg":1100,"added_sugars_tsp_eq":4})).unwrap()
}
#[test]
fn hei_official_thresholds_zero_fat_missing_and_density_scaling() {
    use helpyourself::wellness::diet::score;
    let mut totals = complete_diet_totals();
    let first = score(&totals).unwrap();
    assert_eq!(first["total"], 100.0);
    assert_eq!(first["components"].as_array().unwrap().len(), 13);
    let scaled = totals.iter().map(|(k, v)| (k.clone(), v * 7.0)).collect();
    assert!((score(&scaled).unwrap()["total"].as_f64().unwrap() - 100.0).abs() < 1e-9);
    totals.insert("saturated_fat_g".into(), 0.0);
    totals.insert("unsaturated_fat_g".into(), 0.0);
    assert_eq!(score(&totals).unwrap()["total"], 90.0);
    totals.insert("unsaturated_fat_g".into(), 1.0);
    assert_eq!(score(&totals).unwrap()["total"], 100.0);
    totals.remove("sodium_mg");
    assert!(score(&totals).unwrap()["total"].is_null());
    totals.insert("energy_kcal".into(), f64::NAN);
    assert!(score(&totals).is_err());
}
#[tokio::test]
async fn diet_windows_require_complete_days_and_reject_duplicate_assessments() {
    use helpyourself::wellness::{diet::Request, entries::SaveRequest};
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "diet-owner").await;
    let request = || Request {
        date: "2026-10-05".into(),
        timezone: "UTC".into(),
    };
    let make = || {
        serde_json::from_value::<SaveRequest>(json!({"record_id":uuid::Uuid::new_v4(),"batch_id":uuid::Uuid::new_v4(),"version":1,"at":1791201600i64,"timezone":"UTC","entry":{"kind":"diet_quality","content":{"totals":complete_diet_totals(),"source":"Synthetic confirmed FPED equivalent fixture","complete_day":true,"age_two_or_older":true,"note":""}}})).unwrap()
    };
    state
        .database
        .save_wellness_entry(&user.user_id, make())
        .await
        .unwrap();
    let result = state
        .database
        .diet_quality(&user.user_id, request())
        .await
        .unwrap();
    assert_eq!(result["windows"][0]["result"]["total"], 100.0);
    assert!(result["windows"][1]["result"]["total"].is_null());
    state
        .database
        .save_wellness_entry(&user.user_id, make())
        .await
        .unwrap();
    let result = state
        .database
        .diet_quality(&user.user_id, request())
        .await
        .unwrap();
    assert!(result["windows"][0]["result"]["total"].is_null());
    assert_eq!(result["windows"][0]["conflicting_days"], 1);
}

#[tokio::test]
async fn meal_glucose_alignment_preserves_missing_sources_and_owner_boundaries() {
    use helpyourself::wellness::{entries::SaveRequest, meal_glucose::Request};
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "meal-owner").await;
    let other = crate::add_user(&state, "meal-other").await;
    let entry:SaveRequest=serde_json::from_value(json!({"record_id":uuid::Uuid::new_v4(),"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC","entry":{"kind":"nutrition","content":{"food":"Recorded meal","meal":"lunch","energy_kcal":null,"protein_g":null,"carbohydrate_g":20,"fat_g":null,"fiber_g":null,"water_ml":null,"micronutrients":{},"origin":null,"note":""}}})).unwrap();
    state
        .database
        .save_wellness_entry(&user.user_id, entry)
        .await
        .unwrap();
    let request = || Request {
        start_at: 0,
        end_at: 86400,
        maximum_gap_seconds: 900,
    };
    let result = state
        .database
        .meal_glucose(&user.user_id, request())
        .await
        .unwrap();
    assert_eq!(result["events"].as_array().unwrap().len(), 1);
    assert!(result["series"]["sources"].as_array().unwrap().is_empty());
    assert!(
        state
            .database
            .meal_glucose(&other.user_id, request())
            .await
            .unwrap()["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .database
            .meal_glucose(
                &user.user_id,
                Request {
                    end_at: 8 * 86400,
                    ..request()
                }
            )
            .await
            .is_err()
    );
}

#[test]
fn shopping_uses_recipe_yield_and_preserves_source_distinctions() {
    use helpyourself::wellness::meals::shopping;
    let a = json!({"record_id":"a","version":1,"entry":{"content":{"recipe":{"name":"Soup","servings":4,"ingredients":[{"food_name":"Beans","grams":200,"source":"Label A","nutrients_per_100g":{}}],"instructions":"","note":""},"planned_servings":2,"status":"planned"}}});
    let mut b = a.clone();
    b["record_id"] = json!("b");
    b["entry"]["content"]["planned_servings"] = json!(4);
    let mut skipped = a.clone();
    skipped["entry"]["content"]["status"] = json!("skipped");
    let result = shopping(&[a.clone(), b.clone(), skipped]).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["grams"], 300.0);
    assert_eq!(result[0]["inputs"].as_array().unwrap().len(), 2);
    b["entry"]["content"]["recipe"]["ingredients"][0]["source"] = json!("Label B");
    assert_eq!(shopping(&[a, b]).unwrap().len(), 2);
}

#[test]
fn training_windows_do_not_combine_active_and_elapsed_protocols() {
    use helpyourself::wellness::training::summarize;
    let mut entries:Vec<_>=(0..28).map(|day|json!({"record_id":format!("day-{day}"),"version":1,"at":day*86400+3600,"timezone":"UTC","entry":{"kind":"training_day","content":{"status":"rest"}}})).collect();
    for (day, basis) in [
        (26, "active_excluding_pauses"),
        (27, "elapsed_including_pauses"),
    ] {
        entries[day]["entry"]["content"]["status"] = json!("all_sessions_logged");
        entries.push(json!({"record_id":basis,"version":1,"at":day*86400+7200,"timezone":"UTC","entry":{"kind":"training"},"calculation":{"session_load_au":300,"duration_basis":basis}}));
    }
    let result = summarize(&entries, 0, 28 * 86400, chrono_tz::UTC).unwrap();
    assert_eq!(result["days"][27]["load_au"], 300.0);
    assert!(result["windows"][0]["total_load_au"].is_null());
    assert!(result["windows"][0]["observed_sum_au"].is_null());
    assert_eq!(
        result["windows"][0]["protocol_state"],
        "incompatible_duration_protocols"
    );
    entries[29]["at"] = json!(26 * 86400 + 7300);
    let same_day = summarize(&entries, 0, 28 * 86400, chrono_tz::UTC).unwrap();
    assert_eq!(
        same_day["days"][26]["state"],
        "incompatible_duration_protocols"
    );
}

#[test]
fn hrv_rejects_mismatched_archive_duration() {
    let row = helpyourself::health::StoredHealthRecord {
        platform: "health_connect".into(),
        source_id: "reviewed-device".into(),
        record_id: "duration-mismatch".into(),
        record_type: "nn_intervals".into(),
        start_at: 0,
        end_at: 10,
        version: 1,
        payload_json: json!({"payload":nn_payload()}).to_string(),
    };
    let result = helpyourself::wellness::hrv::summarize(vec![row], chrono_tz::UTC).unwrap();
    assert_eq!(result[0]["windows"][0]["state"], "ineligible_window");
    assert!(result[0]["days"].as_array().unwrap().is_empty());
}

#[test]
fn public_food_lookup_preserves_raw_data_units_and_requires_review() {
    use helpyourself::wellness::{entries::Entry, food_lookup::project};
    let source = json!({"status":1,"product":{"code":"12345678","product_name":"Synthetic oats","brands":"Fixture","nutriments":{"energy-kcal_100g":370,"sodium_100g":0.002,"vitamin-d_100g":0.000001,"proteins_100g":"unknown"},"future_field":{"retain":true}}});
    let result = project("12345678", source.clone(), 100).unwrap();
    let mut entry = result["candidate"].clone();
    assert_eq!(entry["content"]["external_source"]["response"], source);
    assert_eq!(entry["content"]["nutrients_per_100g"]["sodium_mg"], 2.0);
    assert_eq!(entry["content"]["nutrients_per_100g"]["vitamin_d_ug"], 1.0);
    assert!(entry["content"]["nutrients_per_100g"]["protein_g"].is_null());
    assert!(
        serde_json::from_value::<Entry>(entry.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    entry["content"]["external_source"]["mass_basis_confirmed"] = json!(true);
    assert!(
        serde_json::from_value::<Entry>(entry)
            .unwrap()
            .validate()
            .is_ok()
    );
    assert!(project("123/45678", source, 100).is_err());
    assert!(project("12345678", json!({"status":0}), 100).is_err());
}

#[tokio::test]
async fn public_food_lookup_rejects_nonbarcode_paths_before_network_access() {
    use helpyourself::{
        error::AppError,
        wellness::food_lookup::{Request, lookup},
    };
    assert!(matches!(
        lookup(Request {
            barcode: "../../user/data".into()
        })
        .await,
        Err(AppError::Invalid(_))
    ));
}

#[tokio::test]
#[ignore = "Explicit three-year query benchmark writes a large synthetic SQLite fixture"]
async fn three_year_minute_history_query_benchmark() {
    use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
    use std::time::Instant;
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "history-benchmark").await;
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(state.config.data_dir.join("database.sqlite"))
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let count = 3 * 365 * 24 * 60i64;
    let end = 1791244800i64;
    let seeded = Instant::now();
    sqlx::query("WITH RECURSIVE samples(i) AS (SELECT 0 UNION ALL SELECT i+1 FROM samples WHERE i+1<?) INSERT INTO health_records(user_id,platform,source_id,record_id,record_type,start_at,end_at,version,deleted,payload_json) SELECT ?, 'apple_health','synthetic-watch', CAST(i AS TEXT),'heart_rate',?+i*60,?+i*60,1,0,'{\"payload\":{\"value\":60,\"unit\":\"count/min\"}}' FROM samples")
        .bind(count).bind(&user.user_id).bind(end-count*60).bind(end-count*60).execute(&mut db).await.unwrap();
    let seed_seconds = seeded.elapsed().as_secs_f64();
    let request = || DayRequest {
        date: "2026-10-05".into(),
        timezone: "UTC".into(),
        source_priority: BTreeMap::new(),
    };
    let start = Instant::now();
    let result = state
        .database
        .wellness_day(&user.user_id, request())
        .await
        .unwrap();
    let cold_ms = start.elapsed().as_secs_f64() * 1000.0;
    let heart = result["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["record_type"] == "heart_rate")
        .unwrap();
    assert!((heart["current"]["selected"]["value"].as_f64().unwrap() - 60.0).abs() < 1e-9);
    assert_eq!(heart["baseline_valid_days"], 28);
    let mut warm_ms = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        assert_eq!(
            state
                .database
                .wellness_day(&user.user_id, request())
                .await
                .unwrap(),
            result
        );
        warm_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    warm_ms.sort_by(f64::total_cmp);
    let p95 = warm_ms[18];
    eprintln!(
        "THREE_YEAR_BENCHMARK {}",
        json!({"rows":count,"sample_period_seconds":60,"history_days":1095,"seed_seconds":seed_seconds,"cold_day_ms":cold_ms,"warm_day_p95_ms":p95,"warm_day_max_ms":warm_ms[19],"warm_repetitions":20,"scope":"synthetic query fixture, no device sync or original-file benchmark"})
    );
    assert!(
        p95 <= 500.0,
        "Cached daily p95 exceeded the 500 ms budget: {p95}"
    );
}

#[tokio::test]
async fn weekly_query_chunks_preserve_sleep_across_the_chunk_boundary() {
    use helpyourself::health::{
        AggregateRequest, ConnectionRequest, HealthRecordInput, SyncRequest,
    };
    let (_directory, state) = crate::fixture().await;
    let user = crate::add_user(&state, "chunk-sleep").await;
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    state
        .database
        .sync_health(
            &user.user_id,
            SyncRequest {
                connection_id: connection.connection_id,
                batch_id: uuid::Uuid::new_v4().to_string(),
                record_type: "sleep".into(),
                coverage_status: "observed".into(),
                records: vec![HealthRecordInput {
                    record_id: "cross-boundary".into(),
                    source_id: "watch".into(),
                    record_type: "sleep".into(),
                    start_at: 7 * 86400 - 3600,
                    end_at: 7 * 86400 + 3600,
                    version: 1,
                    deleted: false,
                    payload: json!({"category":3}),
                }],
            },
        )
        .await
        .unwrap();
    let result = state
        .database
        .aggregate_health(
            &user.user_id,
            AggregateRequest {
                record_type: "sleep".into(),
                start_date: "1970-01-01".into(),
                end_date: "1970-01-14".into(),
                timezone: "UTC".into(),
            },
        )
        .await
        .unwrap();
    let observed: Vec<_> = result["days"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["value"].is_number())
        .collect();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0]["date"], "1970-01-07");
    assert_eq!(observed[1]["date"], "1970-01-08");
    assert_eq!(observed[0]["value"], 3600.0);
    assert_eq!(observed[1]["value"], 3600.0);
}

#[test]
fn cycle_predictions_preserve_source_uncertainty_and_do_not_become_observations() {
    use helpyourself::wellness::entries::Entry;
    let mut value = json!({"kind":"cycle_prediction","content":{"start_date":"2026-10-08","end_date":"2026-10-12","generated_at":1791201600i64,"source":"Synthetic external forecast v1","uncertainty":"Accuracy unknown","note":""}});
    let entry: Entry = serde_json::from_value(value.clone()).unwrap();
    assert!(entry.validate().is_ok());
    assert_eq!(entry.kind(), "cycle_prediction");
    value["content"]["end_date"] = json!("2026-10-01");
    assert!(
        serde_json::from_value::<Entry>(value.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    value["content"]["end_date"] = json!("2026-10-12");
    value["content"]["uncertainty"] = json!("");
    assert!(
        serde_json::from_value::<Entry>(value)
            .unwrap()
            .validate()
            .is_err()
    );
    let context:Entry=serde_json::from_value(json!({"kind":"cycle","content":{"flow":"none","symptoms":[],"context":"perimenopause","note":""}})).unwrap();
    assert!(context.validate().is_ok());
}
