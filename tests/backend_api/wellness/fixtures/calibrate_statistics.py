"""Measure nominal HAC interval coverage on predeclared synthetic AR(1) scenarios.

This exercise does not choose lag parameters from observed significance.
The tested protocol fixes Bartlett lag 7, 90 days, and Student t inference.
Run with build/statistics-reference/bin/python from the repository root.
"""
import json
from pathlib import Path
import numpy as np
from scipy.stats import t
from statsmodels.stats.proportion import proportion_confint

rng = np.random.default_rng(20261007)
scenarios = []
for rho in [0.0, 0.3, 0.6, 0.9]:
    for missing in [0.0, 0.2]:
        rejected = 0
        trials = 1000
        for _ in range(trials):
            x, error = rng.normal(size=(2, 90))
            for day in range(1, 90):
                x[day] += rho * x[day - 1]
                error[day] += rho * error[day - 1]
            days = np.arange(90)
            design = np.column_stack([np.ones(90), x, days/90] + [(days % 7 == d).astype(float) for d in range(1, 7)])
            keep = rng.random(90) >= missing
            matrix = design[keep]
            target = error[keep]
            n, p = matrix.shape
            beta = np.linalg.lstsq(matrix, target, rcond=None)[0]
            inverse = np.linalg.inv(matrix.T @ matrix)
            residual = target - matrix @ beta
            scores = np.zeros((90, p))
            scores[keep] = matrix * residual[:, None]
            meat = scores.T @ scores
            for lag in range(1, 8):
                cross = scores[lag:].T @ scores[:-lag]
                meat += (1 - lag / 8) * (cross + cross.T)
            variance = (inverse @ meat @ inverse)[1, 1] * n / (n - p)
            rejected += abs(beta[1]) > t.ppf(0.975, n-p) * np.sqrt(variance)
        low, high = proportion_confint(rejected, trials, method='wilson')
        scenarios.append(dict(ar1=rho, missing_fraction=missing, trials=trials, null_rejections=int(rejected),
                              nominal_005_rejection_rate=rejected/trials, wilson_95=[float(low), float(high)]))
output = dict(seed=20261007, protocol='90 calendar days, Bartlett HAC lag 7, n/(n-p), t(n-p), intercept + predictor + trend + weekday',
              purpose='Describe finite-sample limitations. This is not a power study or clinical validation.', scenarios=scenarios)
Path('build/statistics-calibration.json').write_text(json.dumps(output, indent=2) + '\n')
print(json.dumps(output, indent=2))
