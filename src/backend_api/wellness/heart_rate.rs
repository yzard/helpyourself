//! Time-weighted heart-rate bins. No duration is inferred beyond the last sample.
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredMaximum {
    pub bpm: f64,
    pub source: String,
}
impl DeclaredMaximum {
    pub fn validate(&self) -> Result<(), AppError> {
        if !self.bpm.is_finite()
            || !(50.0..=300.0).contains(&self.bpm)
            || self.source.trim().is_empty()
            || self.source.len() > 256
        {
            return Err(AppError::Invalid(
                "Declare a maximum heart rate from 50 to 300 bpm and its source",
            ));
        }
        Ok(())
    }
}
pub fn summarize(
    samples: &[(i64, f64)],
    start: i64,
    end: i64,
    gap: i64,
    maximum: Option<&DeclaredMaximum>,
) -> Result<Value, AppError> {
    if end <= start
        || i128::from(end) - i128::from(start) > 90 * 86400
        || !(1..=1800).contains(&gap)
        || samples.len() > 50000
    {
        return Err(AppError::Invalid(
            "Invalid heart-rate window or sampling gap",
        ));
    }
    if let Some(maximum) = maximum {
        maximum.validate()?;
    }
    let mut previous = None;
    for &(at, value) in samples {
        if at < start
            || at >= end
            || previous.is_some_and(|p| at <= p)
            || !value.is_finite()
            || value <= 0.0
            || value > 300.0
        {
            return Err(AppError::Invalid(
                "Heart-rate samples need increasing timestamps and valid bpm",
            ));
        }
        previous = Some(at);
    }
    let (mut seconds, mut bins, mut zones, mut below, mut above) =
        (0_i64, [0_i64; 6], [0_i64; 5], 0_i64, 0_i64);
    for pair in samples.windows(2) {
        let span = pair[1].0 - pair[0].0;
        if span > gap {
            continue;
        }
        seconds += span;
        let value = pair[0].1;
        bins[((value / 50.0).floor() as usize).min(5)] += span;
        if let Some(maximum) = maximum {
            if value < maximum.bpm * 0.5 {
                below += span;
            } else if value > maximum.bpm {
                above += span;
            } else {
                let index = [0.6, 0.7, 0.8, 0.9]
                    .iter()
                    .take_while(|boundary| value >= maximum.bpm * **boundary)
                    .count();
                zones[index] += span;
            }
        }
    }
    let above_samples = maximum.map_or(0, |maximum| {
        samples.iter().filter(|(_, v)| *v > maximum.bpm).count()
    });
    let load = if maximum.is_some() && seconds > 0 && above_samples == 0 {
        Some(
            zones
                .iter()
                .enumerate()
                .map(|(i, s)| *s as f64 / 60.0 * (i + 1) as f64)
                .sum::<f64>(),
        )
    } else {
        None
    };
    Ok(
        json!({"algorithm_version":"edwards-left-hold-v1","covered_seconds":seconds,"coverage_fraction":seconds as f64/(end-start)as f64,"maximum_gap_seconds":gap,"absolute_bins":bins.iter().enumerate().map(|(i,s)|json!({"lower_bpm":i*50,"upper_bpm":(i+1)*50,"seconds":s,"upper_inclusive":i==5})).collect::<Vec<_>>(),"declared_maximum":maximum,"zone_seconds":maximum.map(|_|zones),"below_50_percent_seconds":maximum.map(|_|below),"above_maximum_seconds":maximum.map(|_|above),"above_maximum_samples":above_samples,"edwards_load_au":load,"state":if maximum.is_none(){"absolute_bins_only"}else if above_samples>0{"maximum_exceeded"}else if seconds==0{"insufficient_coverage"}else{"observed"},"notes":["Intervals hold the earlier sample only up to the next sample within the declared gap. The final sample has no inferred duration.","Relative zones are 50–60, 60–70, 70–80, 80–90 and 90–100 percent of the declared maximum. Lower boundaries are inclusive, and 100 percent is in the final zone.","A value above the declared maximum stops the Edwards load calculation. No age formula supplies a missing maximum.","These are descriptive observed intervals, not a validated estimate of an entire workout or injury risk."]}),
    )
}
