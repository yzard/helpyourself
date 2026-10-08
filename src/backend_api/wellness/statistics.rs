//! OLS with a calendar-distance HAC covariance and fixed moving-block sensitivity analysis.
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde_json::{Value, json};
use statrs::distribution::{ContinuousCDF, StudentsT};

pub const HAC_LAG: usize = 7;
pub const BLOCK_DAYS: usize = 7;
pub const BOOTSTRAPS: usize = 500;
#[derive(Clone)]
pub struct Row {
    pub day: usize,
    pub x: Vec<f64>,
    pub y: f64,
}
struct Fit {
    beta: Vec<f64>,
    residuals: Vec<f64>,
    inverse: Vec<Vec<f64>>,
}
fn fit(rows: &[Row]) -> Option<Fit> {
    let n = rows.len();
    let p = rows.first()?.x.len();
    if p == 0
        || p > 13
        || n <= p
        || rows
            .iter()
            .any(|r| r.x.len() != p || !r.y.is_finite() || r.x.iter().any(|v| !v.is_finite()))
    {
        return None;
    }
    let mut q = vec![vec![0.0; n]; p];
    let mut upper = vec![vec![0.0; p]; p];
    for j in 0..p {
        let mut column: Vec<_> = rows.iter().map(|row| row.x[j]).collect();
        // Reorthogonalization avoids normal-equation squaring of the condition number.
        for _ in 0..2 {
            for k in 0..j {
                let projection: f64 = q[k].iter().zip(&column).map(|(a, b)| a * b).sum();
                upper[k][j] += projection;
                for (value, axis) in column.iter_mut().zip(&q[k]) {
                    *value -= projection * axis;
                }
            }
        }
        let norm = column.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !norm.is_finite() || norm < 1e-8 {
            return None;
        }
        upper[j][j] = norm;
        for (dest, value) in q[j].iter_mut().zip(column) {
            *dest = value / norm;
        }
    }
    let mut beta: Vec<f64> = q
        .iter()
        .map(|column| column.iter().zip(rows).map(|(a, b)| a * b.y).sum())
        .collect();
    for j in (0..p).rev() {
        for k in j + 1..p {
            beta[j] -= upper[j][k] * beta[k];
        }
        beta[j] /= upper[j][j];
    }
    let mut inv_r = vec![vec![0.0; p]; p];
    for (col, column) in inv_r.iter_mut().enumerate() {
        for j in (0..p).rev() {
            let mut value = if j == col { 1.0 } else { 0.0 };
            for k in j + 1..p {
                value -= upper[j][k] * column[k];
            }
            column[j] = value / upper[j][j];
        }
    }
    let mut inverse = vec![vec![0.0; p]; p];
    for j in 0..p {
        for k in 0..p {
            inverse[j][k] = inv_r.iter().map(|column| column[j] * column[k]).sum();
        }
    }
    let residuals = rows
        .iter()
        .map(|r| r.y - r.x.iter().zip(&beta).map(|(a, b)| a * b).sum::<f64>())
        .collect();
    Some(Fit {
        beta,
        residuals,
        inverse,
    })
}
pub fn regress(rows: &[Row], seed: u64) -> Option<Value> {
    if rows.len() < 60
        || rows.len() > 90
        || rows.windows(2).any(|p| p[0].day >= p[1].day)
        || rows.iter().any(|r| r.day >= 90)
    {
        return None;
    }
    let fit_result = fit(rows)?;
    let n = rows.len();
    let p = fit_result.beta.len();
    if p < 2 {
        return None;
    }
    let mut meat = vec![vec![0.0; p]; p];
    for (i, a) in rows.iter().enumerate() {
        for (j, b) in rows.iter().enumerate() {
            let lag = a.day.abs_diff(b.day);
            if lag > HAC_LAG {
                continue;
            }
            let weight = 1.0 - lag as f64 / (HAC_LAG + 1) as f64;
            for (k, meat_row) in meat.iter_mut().enumerate() {
                for (l, value) in meat_row.iter_mut().enumerate() {
                    *value += weight
                        * a.x[k]
                        * fit_result.residuals[i]
                        * b.x[l]
                        * fit_result.residuals[j];
                }
            }
        }
    }
    let mut variance = 0.0;
    for (j, meat_row) in meat.iter().enumerate() {
        for (k, value) in meat_row.iter().enumerate() {
            variance += fit_result.inverse[1][j] * value * fit_result.inverse[k][1];
        }
    }
    variance *= n as f64 / (n - p) as f64;
    if !variance.is_finite() || variance <= 1e-16 {
        return None;
    }
    let se = variance.sqrt();
    let distribution = StudentsT::new(0.0, 1.0, (n - p) as f64).ok()?;
    let critical = distribution.inverse_cdf(0.975);
    let effect = fit_result.beta[1];
    let p_value = (2.0 * distribution.sf((effect / se).abs())).clamp(0.0, 1.0);
    let mut rng = StdRng::seed_from_u64(seed);
    let mut boot = Vec::with_capacity(BOOTSTRAPS);
    let mut calendar: Vec<Option<&Row>> = vec![None; 90];
    for row in rows {
        calendar[row.day] = Some(row);
    }
    for _ in 0..BOOTSTRAPS {
        let mut sampled = Vec::new();
        let mut slots = 0;
        while slots < 90 {
            let from = rng.gen_range(0..=90 - BLOCK_DAYS);
            for row in &calendar[from..from + BLOCK_DAYS] {
                if slots >= 90 {
                    break;
                }
                if let Some(row) = row {
                    sampled.push((*row).clone());
                }
                slots += 1;
            }
        }
        if let Some(fitted) = fit(&sampled) {
            let b = fitted.beta[1];
            if b.is_finite() {
                boot.push(b);
            }
        }
    }
    boot.sort_by(f64::total_cmp);
    let bootstrap = if boot.len() >= 450 {
        Some([quantile(&boot, 0.025), quantile(&boot, 0.975)])
    } else {
        None
    };
    Some(
        json!({"effect":effect,"standard_error":se,"ci_95":[effect-critical*se,effect+critical*se],"p_value":p_value,"bootstrap_ci_95":bootstrap,"bootstrap_successes":boot.len(),"sample_days":n,"parameters":p,"residual_degrees_of_freedom":n-p}),
    )
}
fn quantile(values: &[f64], q: f64) -> f64 {
    let at = q * (values.len() - 1) as f64;
    let lo = at.floor() as usize;
    let hi = at.ceil() as usize;
    values[lo] + (values[hi] - values[lo]) * (at - lo as f64)
}
pub fn benjamini_yekutieli(p: &[Option<f64>]) -> Vec<Option<f64>> {
    let m = p.len();
    let harmonic: f64 = (1..=m).map(|n| 1.0 / n as f64).sum();
    let mut order: Vec<_> = p
        .iter()
        .enumerate()
        .map(|(i, p)| (i, p.unwrap_or(1.0)))
        .collect();
    order.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut q = vec![None; m];
    let mut ceiling = 1.0_f64;
    for (rank, (index, value)) in order.iter().enumerate().rev() {
        ceiling = ceiling.min(value * m as f64 * harmonic / (rank + 1) as f64);
        if p[*index].is_some() {
            q[*index] = Some(ceiling);
        }
    }
    q
}
