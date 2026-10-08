//! Public formulas with explicit validity conditions. These are not clinical scores.
use crate::error::AppError;
use serde::{Deserialize, Serialize};

pub fn median(values: &mut [f64]) -> Result<f64, AppError> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
        return Err(AppError::Invalid("Finite observations required"));
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Ok(if values.len().is_multiple_of(2) {
        values[middle - 1] / 2.0 + values[middle] / 2.0
    } else {
        values[middle]
    })
}

pub fn session_load(rpe: f64, minutes: f64) -> Result<f64, AppError> {
    if !rpe.is_finite()
        || !(0.0..=10.0).contains(&rpe)
        || !minutes.is_finite()
        || !(0.0..=1440.0).contains(&minutes)
    {
        return Err(AppError::Invalid(
            "CR10 effort and duration from 0 to 1440 minutes required",
        ));
    }
    Ok(rpe * minutes)
}

pub fn rmssd(nn_ms: &[f64]) -> Result<f64, AppError> {
    if nn_ms.len() < 2
        || nn_ms.len() > 200000
        || nn_ms
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0 || *v > 10000.0)
    {
        return Err(AppError::Invalid(
            "Continuous quality-reviewed NN intervals required",
        ));
    }
    Ok(
        (nn_ms.windows(2).map(|v| (v[1] - v[0]).powi(2)).sum::<f64>() / (nn_ms.len() - 1) as f64)
            .sqrt(),
    )
}

/// Exactly seven complete days at one-minute resolution; None is unknown, not awake.
pub fn sleep_regularity(states: &[Option<bool>]) -> Result<f64, AppError> {
    if states.len() != 7 * 1440 || states.iter().any(Option::is_none) {
        return Err(AppError::Invalid(
            "Seven complete days of sleep and wake states required",
        ));
    }
    let equal = states
        .iter()
        .zip(states.iter().skip(1440))
        .filter(|(a, b)| a == b)
        .count();
    Ok(200.0 * equal as f64 / (6 * 1440) as f64 - 100.0)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClinicalAge {
    pub age_years: f64,
    pub albumin_g_l: f64,
    pub creatinine_umol_l: f64,
    pub glucose_mmol_l: f64,
    pub crp_mg_dl: f64,
    pub lymphocyte_percent: f64,
    pub mcv_fl: f64,
    pub rdw_percent: f64,
    pub alp_u_l: f64,
    pub wbc_1000_ul: f64,
}
impl ClinicalAge {
    pub fn calculate(&self) -> Result<f64, AppError> {
        let values = [
            self.age_years,
            self.albumin_g_l,
            self.creatinine_umol_l,
            self.glucose_mmol_l,
            self.crp_mg_dl,
            self.lymphocyte_percent,
            self.mcv_fl,
            self.rdw_percent,
            self.alp_u_l,
            self.wbc_1000_ul,
        ];
        if values.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || self.age_years < 18.0
            || self.age_years > 120.0
            || self.lymphocyte_percent > 100.0
            || self.rdw_percent > 100.0
        {
            return Err(AppError::Invalid(
                "Complete reviewed adult clinical inputs required",
            ));
        }
        let xb = -19.907 - 0.0336 * self.albumin_g_l
            + 0.0095 * self.creatinine_umol_l
            + 0.1953 * self.glucose_mmol_l
            + 0.0954 * self.crp_mg_dl.ln()
            - 0.012 * self.lymphocyte_percent
            + 0.0268 * self.mcv_fl
            + 0.3306 * self.rdw_percent
            + 0.00188 * self.alp_u_l
            + 0.0554 * self.wbc_1000_ul
            + 0.0804 * self.age_years;
        // Evaluate in log space, including extreme inputs, without forming mortality probability.
        let result = 141.50 + ((0.00553_f64 * 1.51714 / 0.0076927).ln() + xb) / 0.09165;
        if !result.is_finite() {
            return Err(AppError::Invalid("Clinical inputs overflow"));
        }
        Ok(result)
    }
}

#[derive(Serialize)]
pub struct GlucoseSummary {
    pub covered_seconds: i64,
    pub coverage_fraction: f64,
    pub mean_mg_dl: Option<f64>,
    pub coefficient_of_variation: Option<f64>,
    pub tir_percent: Option<f64>,
    pub below_70_percent: Option<f64>,
    pub below_54_percent: Option<f64>,
    pub above_180_percent: Option<f64>,
    pub above_250_percent: Option<f64>,
}
/// Left-held samples only over short gaps, clipped to the query window. Last sample has no inferred duration.
pub fn glucose(
    samples: &[(i64, f64)],
    start: i64,
    end: i64,
    maximum_gap_seconds: i64,
) -> Result<GlucoseSummary, AppError> {
    if end <= start
        || i128::from(end) - i128::from(start) > 366 * 86400
        || !(1..=1800).contains(&maximum_gap_seconds)
        || samples.len() > 200000
        || samples
            .iter()
            .any(|(_, v)| !v.is_finite() || *v <= 0.0 || *v > 2000.0)
        || samples.windows(2).any(|w| w[0].0 >= w[1].0)
    {
        return Err(AppError::Invalid(
            "Invalid glucose series or coverage window",
        ));
    }
    let mut seconds = 0_i64;
    let mut sum = 0.0;
    let mut sum_squared = 0.0;
    let mut ranges = [0.0; 5];
    for pair in samples.windows(2) {
        let gap = i128::from(pair[1].0) - i128::from(pair[0].0);
        if gap > i128::from(maximum_gap_seconds) {
            continue;
        }
        let span =
            (i128::from(pair[1].0.min(end)) - i128::from(pair[0].0.max(start))).max(0) as i64;
        let value = pair[0].1;
        seconds += span;
        sum += value * span as f64;
        sum_squared += value * value * span as f64;
        for (i, inside) in [
            (70.0..=180.0).contains(&value),
            value < 70.0,
            value < 54.0,
            value > 180.0,
            value > 250.0,
        ]
        .into_iter()
        .enumerate()
        {
            if inside {
                ranges[i] += span as f64;
            }
        }
    }
    let mean = if seconds > 0 {
        Some(sum / seconds as f64)
    } else {
        None
    };
    let percentage = |i: usize| {
        if seconds > 0 {
            Some(100.0 * ranges[i] / seconds as f64)
        } else {
            None
        }
    };
    Ok(GlucoseSummary {
        covered_seconds: seconds,
        coverage_fraction: seconds as f64 / (end - start) as f64,
        mean_mg_dl: mean,
        coefficient_of_variation: mean
            .map(|m| 100.0 * (sum_squared / seconds as f64 - m * m).max(0.0).sqrt() / m),
        tir_percent: percentage(0),
        below_70_percent: percentage(1),
        below_54_percent: percentage(2),
        above_180_percent: percentage(3),
        above_250_percent: percentage(4),
    })
}
