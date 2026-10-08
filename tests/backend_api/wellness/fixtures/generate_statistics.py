"""Generate fixed regression references with statsmodels, NumPy and SciPy.

Run with the isolated statistics-reference environment under build/.
These libraries are validation dependencies, not server dependencies.
"""
import json
from pathlib import Path

import numpy as np
import scipy.stats
import statsmodels.api as sm
from statsmodels.stats.sandwich_covariance import cov_hac, S_hac_simple
from statsmodels.stats.multitest import multipletests

rng = np.random.default_rng(20261006)
days = np.arange(90)
x = (rng.random(90) > 0.5).astype(float)
noise = rng.normal(size=90)
for day in range(1, 90):
    noise[day] += 0.45 * noise[day - 1]
design = np.column_stack([np.ones(90), x, days / 90] + [(days % 7 == weekday).astype(float) for weekday in range(1, 7)])
y = 2 + 0.7 * x + 0.03 * days + 0.2 * (days % 7) + noise
cases = []
for omitted in [[], [4, 12, 13, 22, 45, 66, 80]]:
    keep = np.array([day not in omitted for day in days])
    model = sm.OLS(y[keep], design[keep]).fit()
    n, p = model.model.exog.shape
    scores = np.zeros((90, p))
    scores[keep] = model.model.exog * model.resid[:, None]
    covariance = model.normalized_cov_params @ S_hac_simple(scores, nlags=7) @ model.normalized_cov_params * n / (n - p)
    if not omitted:
        np.testing.assert_allclose(covariance, cov_hac(model, nlags=7), rtol=1e-12, atol=1e-12)
    effect, se = model.params[1], np.sqrt(covariance[1, 1])
    critical = scipy.stats.t.ppf(0.975, n-p)
    cases.append(dict(name='calendar_gaps' if omitted else 'complete', rows=[dict(day=int(day), x=design[day].tolist(), y=float(y[day])) for day in days[keep]], expected=dict(effect=float(effect), standard_error=float(se), ci_95=[float(effect-critical*se), float(effect+critical*se)], p_value=float(2*scipy.stats.t.sf(abs(effect/se), n-p)))))
result = dict(reference='statsmodels 0.14.6 OLS, Bartlett HAC(7), calendar-zero-padded scores, n/(n-p), SciPy Student t', cases=cases,
              by=dict(p_values=[0.001, 0.03, 0.2, None], q_values=[*multipletests([0.001,0.03,0.2,1.0], method='fdr_by')[1][:3], None]))
Path(__file__).with_name('statistics.json').write_text(json.dumps(result, indent=2) + '\n')
