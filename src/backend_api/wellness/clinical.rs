//! Explicit research calculation from one reviewed laboratory collection.
use super::algorithms::ClinicalAge;
use crate::{database::Database, error::AppError, laboratory::normalized};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClinicalRequest {
    pub report_id: String,
    pub age_at_collection_years: f64,
    pub research_acknowledged: bool,
}
impl Database {
    pub async fn clinical_age(
        &self,
        user: &str,
        request: ClinicalRequest,
    ) -> Result<Value, AppError> {
        if !request.research_acknowledged
            || !request.age_at_collection_years.is_finite()
            || !(20.0..=120.0).contains(&request.age_at_collection_years)
        {
            return Err(AppError::Invalid(
                "Research acknowledgement and adult age at collection are required",
            ));
        }
        let revision = self.data_revision(user).await?;
        self.report_summary(user, &request.report_id).await?;
        let observations = self.confirmed(user).await?;
        let wanted = [
            "albumin",
            "creatinine",
            "glucose",
            "crp",
            "lymphocyte_percent",
            "mcv",
            "rdw",
            "alp",
            "wbc",
        ];
        let mut inputs = BTreeMap::new();
        let mut evidence = Vec::new();
        let mut collection = None;
        for observation in observations
            .iter()
            .filter(|o| o.report_id == request.report_id)
        {
            let Some(metric) = observation
                .payload
                .metric_id
                .as_deref()
                .filter(|m| wanted.contains(m))
            else {
                continue;
            };
            let Some((value, unit)) = normalized(&observation.payload) else {
                return Err(AppError::Invalid(
                    "Every model input must be an exact reviewed value with a supported unit",
                ));
            };
            let date = observation
                .payload
                .sampled_at
                .as_ref()
                .ok_or(AppError::Invalid(
                    "Every model input needs its collection date",
                ))?;
            if collection.as_ref().is_some_and(|d| d != date) {
                return Err(AppError::Invalid(
                    "Model inputs must share the same collection date and precision",
                ));
            }
            collection = Some(date.clone());
            let value = value
                .parse::<f64>()
                .map_err(|_| AppError::Invalid("Invalid model input"))?;
            if inputs.insert(metric.to_string(), value).is_some() {
                return Err(AppError::Invalid(
                    "Resolve duplicate model inputs before calculating",
                ));
            }
            evidence.push(json!({"observation_id":observation.observation_id,"metric_id":metric,"value":value,"unit":unit,"revision":observation.revision}));
        }
        let missing: Vec<_> = wanted
            .iter()
            .filter(|name| !inputs.contains_key(**name))
            .collect();
        if !missing.is_empty() {
            return Ok(
                json!({"state":"insufficient_data","missing_metrics":missing,"value":null,"evidence_status":"research"}),
            );
        }
        let model = ClinicalAge {
            age_years: request.age_at_collection_years,
            albumin_g_l: inputs["albumin"] * 10.0,
            creatinine_umol_l: inputs["creatinine"] * 88.4,
            glucose_mmol_l: inputs["glucose"] * 0.05551,
            crp_mg_dl: inputs["crp"],
            lymphocyte_percent: inputs["lymphocyte_percent"],
            mcv_fl: inputs["mcv"],
            rdw_percent: inputs["rdw"],
            alp_u_l: inputs["alp"],
            wbc_1000_ul: inputs["wbc"],
        };
        let value = model.calculate()?;
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; calculate again"));
        }
        Ok(
            json!({"state":"ready","value":value,"unit":"years","evidence_status":"research","algorithm_version":"clinical-phenoage-2019-correction-v1","conversion_version":crate::laboratory::CONVERSION_VERSION,"data_revision":revision,"computed_at":crate::authentication::now()?,"report_id":request.report_id,"sampled_at":collection,"age_at_collection_years":request.age_at_collection_years,"inputs":evidence,"model_inputs":model,"reference":"https://journals.plos.org/plosmedicine/article?id=10.1371/journal.pmed.1002760","limitations":"A fixed research model, not measured biological age, a diagnosis, or an aging-rate estimate. External performance varies by population and outcome."}),
        )
    }
}
