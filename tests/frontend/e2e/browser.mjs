// Run through tests/docker/deployment.py --webgui PATH/TO/playwright/index.mjs.
// Reports, authentication, health and originals use the real isolated API/Caddy stack.
// AI states are response fixtures; backend model execution has its own Rust tests.
import { pathToFileURL } from 'node:url';
import { mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';

const { chromium } = await import(pathToFileURL(process.argv[2]).href);
const origin = process.env.WEBGUI_TEST_ORIGIN;
const username = process.env.WEBGUI_TEST_USERNAME;
const password = process.env.WEBGUI_TEST_PASSWORD;
const reportID = process.env.WEBGUI_TEST_REPORT;
const pdfID = process.env.WEBGUI_TEST_PDF;
assert.ok(origin && username && password && reportID && pdfID, 'The isolated deployment fixture is required.');
await mkdir('build/webgui/screenshots', { recursive: true });
const browser = await chromium.launch({ headless: true });
let diagnosticPage;
const networkFailures = [];
try {
  const context = await browser.newContext({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage();
  diagnosticPage = page;
  page.on('requestfailed', request => networkFailures.push({ path: new URL(request.url()).pathname, reason: request.failure()?.errorText }));
  const errors = [];
  const forbidden = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error' && /Content Security Policy/.test(message.text())) errors.push(message.text()); });
  page.on('request', request => {
    if (/\/api\/v1\/(files\/upload|reports\/review|health\/(connect|sync)|user\/delete|reports\/delete)/.test(request.url())) forbidden.push(request.url());
  });
  const login = async () => {
    await page.getByLabel('Username', { exact: true }).fill(username);
    await page.getByLabel('Password', { exact: true }).fill(password);
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await page.getByRole('navigation').waitFor();
    await page.getByRole('navigation').getByRole('link', { name: /Archive/ }).click();
    await page.getByRole('heading', { name: 'Report library' }).waitFor();
  };
  const nav = async name => {
    await page.getByRole('navigation').getByRole('link', { name: new RegExp(name) }).click();
    await page.waitForFunction(name => {
      const main = document.querySelector('main');
      return main?.querySelector('h1')?.textContent === name && !main.hasAttribute('aria-busy');
    }, name);
  };
  const screenshot = async filename => {
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.screenshot({ path: `build/webgui/screenshots/${filename}`, fullPage: true });
  };
  const openOriginal = async () => {
    for (let attempt = 0; attempt < 3; attempt++) {
      const previous = networkFailures.length;
      await page.getByRole('button', { name: 'Open original', exact: true }).click();
      try {
        await page.getByRole('dialog').waitFor({ timeout: 5000 });
        return;
      } catch (error) {
        const changed = networkFailures.slice(previous).some(failure => failure.path.endsWith('/download') && failure.reason === 'net::ERR_NETWORK_CHANGED');
        if (!changed || attempt === 2) throw error;
        console.log('Retrying original GET after a host network change.');
      }
    }
  };
  await page.goto(origin);
  await screenshot('login-desktop.png');
  await login();
  await page.getByRole('link', { name: /synthetic.png/ }).click();
  await page.getByRole('heading', { name: 'Synthetic LDL' }).waitFor();
  assert.ok(await page.getByText('Standardized: 120 mg/dL', { exact: true }).isVisible());
  await openOriginal();
  await page.waitForFunction(() => { const image = document.querySelector('dialog img'); return image?.complete && image.naturalWidth > 0; });
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Show revision history' }).click();
  await page.getByText('Observation revisions', { exact: true }).waitFor();
  await nav('Trends');
  await page.getByRole('button', { name: /120 mg\/dL. Open source report/ }).waitFor();
  await screenshot('trends-desktop.png');
  await page.getByRole('button', { name: /120 mg\/dL. Open source report/ }).focus();
  await page.keyboard.press('Enter');
  await page.getByRole('heading', { name: 'Synthetic LDL' }).waitFor();
  await page.goto(`${origin}/#reports/${pdfID}`);
  await page.getByRole('heading', { name: 'text.pdf', exact: true }).waitFor();
  await openOriginal();
  await page.locator('dialog iframe').waitFor();
  assert.ok((await page.locator('dialog iframe').getAttribute('src')).startsWith('blob:'));
  await page.keyboard.press('Escape');
  await page.goto(`${origin}/#health`);
  await page.getByRole('heading', { name: 'Daily view', exact: true }).waitFor();
  await page.getByRole('heading', { name: 'Archived details' }).waitFor();
  assert.ok(await page.getByText('apple_health:synthetic-watch', { exact: true }).first().isVisible());
  await page.getByText(/apple_health · resting_heart_rate/).click();
  assert.ok(await page.getByText(/"record_id": "browser-sample"/).isVisible());
  await screenshot('health-desktop.png');
  await nav('Overview');
  await page.getByRole('heading', { name: 'Resting heart rate', exact: true }).waitFor();
  await screenshot('overview-desktop.png');
  await page.getByText('Customize overview', { exact: true }).click();
  await page.getByRole('checkbox', { name: 'Resting heart rate', exact: true }).check();
  await page.getByLabel('Your daily sleep target (minutes, optional)', { exact: true }).fill('480');
  const savedPreferences = page.waitForResponse(response => response.url().endsWith('/wellness/preferences/save'));
  await page.getByRole('button', { name: 'Save overview preferences', exact: true }).click();
  assert.equal((await savedPreferences).status(), 200);
  await page.waitForFunction(() => document.querySelectorAll('.metric-grid > section').length === 1);
  await page.getByRole('button', { name: 'Open timeline', exact: true }).click();
  await page.getByRole('heading', { name: 'Last 7 days', exact: true }).waitFor();
  await page.getByText(/report_upload · revision/).first().waitFor();
  await nav('Overview');
  await page.getByRole('button', { name: 'Sleep sessions', exact: true }).click();
  await page.getByText('1 hours asleep', { exact: true }).waitFor();
  await page.getByText('Confirm classification or correct your sleep estimate', { exact: true }).click();
  await page.getByLabel('Classification', { exact: true }).selectOption('nap');
  await page.getByLabel('User estimated sleep minutes (optional)', { exact: true }).fill('45');
  await page.getByLabel('Reason or context', { exact: true }).fill('Synthetic sleep correction');
  await page.getByRole('button', { name: 'Save sleep confirmation', exact: true }).click();
  await page.getByText('User estimate: 0.75 hours. Source duration remains unchanged.', { exact: true }).waitFor();
  await page.getByText('1 hours asleep', { exact: true }).waitFor();
  await screenshot('sleep-correction-desktop.png');
  await nav('Overview');
  await page.getByRole('button', { name: 'Record a daily log', exact: true }).click();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Delete log', exact: true }).click();
  await page.getByText('No entries in this range.', { exact: true }).waitFor();
  await nav('Overview');
  await page.getByRole('button', { name: 'Record a daily log', exact: true }).click();
  await page.getByLabel('Log type', { exact: true }).selectOption('training');
  await page.getByLabel('Observed at', { exact: true }).fill('2026-10-05T10:00');
  await page.getByLabel('Session ended at', { exact: true }).fill('2026-10-05T10:45');
  await page.getByLabel('Effort answered at (required with CR10)', { exact: true }).fill('2026-10-05T11:15');
  await page.getByLabel('Paused minutes (enter 0 if none)', { exact: true }).fill('0');
  await page.getByLabel('Activity', { exact: true }).fill('Synthetic strength');
  await page.getByLabel('Duration (minutes)', { exact: true }).fill('45');
  await page.getByLabel('Session effort (CR10, 0–10)', { exact: true }).fill('6');
  await page.getByLabel('Sets: one exercise, repetitions, kg per line', { exact: true }).fill('Squat, 5, 60');
  await page.getByRole('button', { name: 'Save log', exact: true }).click();
  await page.getByText('Session load: 270 AU', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  await page.getByLabel('Session effort (CR10, 0–10)', { exact: true }).fill('8');
  await page.getByRole('button', { name: 'Save revision', exact: true }).click();
  await page.getByText('Session load: 360 AU', { exact: true }).waitFor();
  await screenshot('training-log-desktop.png');
  await nav('Trends');
  await page.getByRole('button', { name: 'Review daily logs', exact: true }).click();
  await page.getByRole('heading', { name: 'Recorded strength bests in this window', exact: true }).waitFor();
  await page.getByRole('cell', { name: '360 AU', exact: true }).waitFor();
  await screenshot('log-review-desktop.png');
  await page.getByRole('button', { name: 'Record or edit a log', exact: true }).click();
  await page.getByText('Session load: 360 AU', { exact: true }).waitFor();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Delete log', exact: true }).click();
  await page.getByText('No entries in this range.', { exact: true }).waitFor();
  await nav('Overview');
  await page.getByRole('button', { name: 'Breathing practice', exact: true }).click();
  await page.getByRole('button', { name: 'Start five minutes', exact: true }).click();
  await page.getByRole('timer').filter({ hasText: '4:59' }).waitFor();
  await page.getByRole('button', { name: 'Stop practice', exact: true }).click();
  await page.getByRole('button', { name: 'Save practice log', exact: true }).click();
  await page.getByText('Practice saved to daily logs.', { exact: true }).waitFor();
  await nav('Overview');
  await page.getByRole('button', { name: 'Record a daily log', exact: true }).click();
  await page.getByRole('button', { name: 'Delete log', exact: true }).waitFor();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Delete log', exact: true }).click();
  await page.getByText('No entries in this range.', { exact: true }).waitFor();
  await page.getByLabel('Log type', { exact: true }).selectOption('journal');
  await page.getByLabel('Behaviors: one name = yes or no per line', { exact: true }).fill('alcohol = no');
  await page.getByLabel('Measurements: name = number unit, one per line', { exact: true }).fill('caffeine = 120 mg');
  await page.getByLabel('Times: name = HH:MM, one per line', { exact: true }).fill('bedtime = 23:40');
  await page.getByRole('button', { name: 'Save log', exact: true }).click();
  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  assert.equal(await page.getByLabel('Times: name = HH:MM, one per line', { exact: true }).inputValue(), 'bedtime = 23:40');
  await nav('Insights');
  await page.getByRole('button', { name: 'Behavior associations', exact: true }).click();
  await page.getByLabel('Outcome', { exact: true }).selectOption('resting_heart_rate');
  await page.getByLabel('Behavior names (one per line, up to 8)', { exact: true }).fill('caffeine\nalcohol');
  await page.getByRole('checkbox').check();
  assert.deepEqual(await page.locator('form').evaluateAll(forms => forms.flatMap(form => [...form.elements].filter(item => item.willValidate && !item.checkValidity()).map(item => ({label: item.getAttribute('aria-label'), value: item.value, reason: item.validationMessage})))), []);
  const analysisResponse = page.waitForResponse(response => response.url().endsWith('/wellness/associations/run'));
  await page.getByRole('button', { name: 'Run declared analysis', exact: true }).click();
  const computedResponse = await analysisResponse; assert.equal(computedResponse.status(), 200);
  const association = await computedResponse.json();
  assert.equal(association.results.length, 2);
  assert.equal(association.calibration.significance_decisions_enabled, false);
  await page.getByText('insufficient days', { exact: true }).first().waitFor();
  await nav('Insights');
  await page.getByRole('button', { name: 'Behavior associations', exact: true }).click();
  await page.getByRole('button', { name: /^resting_heart_rate ·/ }).click();
  await page.getByText('insufficient days', { exact: true }).first().waitFor();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByText('insufficient days', { exact: true }).first().waitFor();
  await page.setViewportSize({ width: 1440, height: 1000 });
  await screenshot('behavior-associations-desktop.png');
  await page.getByText('insufficient days', { exact: true }).first().waitFor();
  await nav('Overview');
  await page.getByRole('button', { name: 'Record a daily log', exact: true }).click();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Delete log', exact: true }).click();
  await page.getByText('No entries in this range.', { exact: true }).waitFor();
  await nav('Insights');
  const remainingResults = page.waitForResponse(response => response.url().endsWith('/wellness/associations/list'));
  await page.getByRole('button', { name: 'Behavior associations', exact: true }).click();
  assert.deepEqual((await (await remainingResults).json()).results, []);
  await nav('Data');
  await page.getByRole('button', { name: 'Create full export', exact: true }).waitFor();
  await screenshot('data-desktop.png');
  const gpx = '<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg><trkpt lat="0" lon="0"><time>2026-01-01T00:00:00Z</time></trkpt><trkpt lat="0" lon="0.001"><time>2026-01-01T00:01:00Z</time></trkpt></trkseg></trk></gpx>';
  await page.getByLabel('Import a GPX, TCX, or FIT activity', { exact: true }).setInputFiles({ name: 'track.gpx', mimeType: 'application/gpx+xml', buffer: Buffer.from(gpx) });
  await page.getByRole('button', { name: 'Import track', exact: true }).click();
  await page.getByText(/track.gpx · /).waitFor();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Delete imported track', exact: true }).click();
  await page.getByText(/track.gpx · /).waitFor({ state: 'detached' });
  const tcx = '<TrainingCenterDatabase xmlns="http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2"><Activities><Activity Sport="Biking"><Id>2026-01-01T00:00:00Z</Id><Lap StartTime="2026-01-01T00:00:00Z"><TotalTimeSeconds>60</TotalTimeSeconds><DistanceMeters>500</DistanceMeters><Calories>10</Calories><Intensity>Active</Intensity><TriggerMethod>Manual</TriggerMethod></Lap></Activity></Activities></TrainingCenterDatabase>';
  await page.getByLabel('Import a GPX, TCX, or FIT activity', { exact: true }).setInputFiles({ name: 'indoor.tcx', mimeType: 'application/xml', buffer: Buffer.from(tcx) });
  await page.getByRole('button', { name: 'Import track', exact: true }).click();
  await page.getByText(/indoor.tcx · /).waitFor();
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Delete imported track', exact: true }).click();
  await page.getByText(/indoor.tcx · /).waitFor({ state: 'detached' });
  await nav('Trends');
  await page.getByRole('button', { name: 'Device trends', exact: true }).click();
  await page.getByText('No recorded measurements in this window.', { exact: true }).waitFor();
  await page.getByLabel('Measurement', { exact: true }).selectOption('heart_rate');
  await page.getByText('absolute bins only', { exact: true }).waitFor();
  await page.getByLabel('Declared maximum heart rate (optional, bpm)', { exact: true }).fill('200');
  await page.getByLabel('Maximum heart rate source', { exact: true }).fill('Synthetic measured maximum');
  await page.getByRole('button', { name: 'Apply sampling protocol', exact: true }).click();
  await page.getByText(/Observed Edwards load:/).waitFor();
  await screenshot('heart-rate-zones-desktop.png');
  await page.getByLabel('Measurement', { exact: true }).selectOption('vo2_max');
  await page.getByText('No recorded measurements in this window.', { exact: true }).waitFor();
  await nav('Insights');
  assert.ok(await page.getByRole('button', { name: 'Run lipid review' }).isDisabled());
  await page.getByRole('button', { name: 'Blood-test research model', exact: true }).click();
  await page.getByLabel('Age at collection (years)', { exact: true }).fill('50');
  await page.getByLabel('I understand that this is a research result', { exact: true }).check();
  await page.getByRole('button', { name: 'Calculate from reviewed inputs', exact: true }).click();
  await page.getByText(/Missing reviewed inputs:/).waitFor();
  await page.emulateMedia({ colorScheme: 'dark' });
  await nav('Overview');
  await screenshot('overview-dark.png');
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole('heading', { name: 'Resting heart rate', exact: true }).waitFor();
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await screenshot('overview-mobile-dark.png');
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.emulateMedia({ colorScheme: 'light' });
  await nav('Archive');
  await page.reload();
  await page.getByRole('heading', { name: 'Welcome back' }).waitFor();

  // Enable only a browser fixture of the existing preset, without changing server config.
  await page.route('**/api/v1/server/status', async route => {
    const response = await route.fetch();
    const data = await response.json(); data.capabilities.analysis = true;
    await route.fulfill({ response, json: data });
  });
  let state = 'queued';
  const original = { observation_id: 'fixture-observation', report_id: reportID, payload: { raw_name: '<img src=x onerror=alert(1)>', raw_result: '120', raw_unit: 'mg/dL' } };
  let createCount = 0;
  await page.route('**/api/v1/analysis/create', async route => {
    const body = route.request().postDataJSON();
    assert.ok(body.start_date && body.end_date && body.timezone);
    createCount++;
    await route.fulfill({ json: { run_id: 'fixture-review' } });
  });
  await page.route('**/api/v1/analysis/get', route => route.fulfill({ json: {
    run_id: 'fixture-review', status: state,
    input: { observations: [original], scope: { timezone: 'UTC' } },
    output: state === 'ready' ? {
      review: { summary: '<script>alert("synthetic")</script>', findings: [{ title: 'Lipid trend', hypothesis: 'A synthetic research finding.', observation_ids: ['fixture-observation'], other_explanations: ['Measurement context'], missing_information: ['Family history'], questions_for_clinician: ['How should this trend be evaluated?'] }] },
      evidence: [{ title: 'MedlinePlus', url: 'https://medlineplus.gov/lab-tests/cholesterol-levels/' }, { title: 'Unsafe link fixture', url: 'javascript:alert(1)' }],
    } : null,
  } }));
  await page.route('**/api/v1/analysis/retry', route => route.fulfill({ json: { queued: true } }));
  await login();
  await nav('Insights');
  await page.getByRole('button', { name: 'Run lipid review' }).click();
  await page.getByText('The server is processing this review.', { exact: false }).waitFor();
  assert.equal(createCount, 1);
  state = 'ready';
  await page.getByRole('button', { name: 'Refresh status' }).click();
  await page.getByRole('heading', { name: 'Lipid trend' }).waitFor();
  assert.ok(await page.getByText('<script>alert("synthetic")</script>', { exact: true }).isVisible());
  assert.equal(await page.locator('main script, main img').count(), 0);
  assert.equal(await page.locator('a[href^="javascript:"]').count(), 0);
  assert.ok(await page.getByRole('link', { name: /<img src=x onerror=alert/ }).isVisible());
  await screenshot('insights-desktop.png');
  state = 'failed';
  await page.getByRole('button', { name: 'Refresh status' }).click();
  await page.getByRole('button', { name: 'Retry review' }).waitFor();
  state = 'queued';
  await page.getByRole('button', { name: 'Retry review' }).click();
  await page.getByText('The server is processing this review.', { exact: false }).waitFor();
  state = 'stale';
  await page.getByRole('button', { name: 'Refresh status' }).click();
  await page.getByText('Source data changed.', { exact: false }).waitFor();

  await page.setViewportSize({ width: 390, height: 844 });
  await nav('Trends');
  await screenshot('trends-mobile.png');
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  assert.equal(await page.locator('input[type=file]').count(), 0);
  assert.deepEqual(await page.evaluate(() => [localStorage.length, sessionStorage.length]), [0, 0]);
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await page.getByRole('heading', { name: 'Welcome back' }).waitFor();
  await screenshot('login-mobile.png');
  await login();
  await page.route('**/api/v1/reports/get', route => route.fulfill({ status: 401, json: { error: { message: 'Synthetic revoked session' } } }));
  await page.getByRole('link', { name: /synthetic.png/ }).click();
  await page.getByRole('heading', { name: 'Welcome back' }).waitFor();
  assert.ok(await page.getByText('Your session has expired. Sign in again.', { exact: true }).isVisible());
  assert.deepEqual(forbidden, []);
  assert.deepEqual(errors, []);
  console.log('Web GUI browser checks passed: real archive/health/auth/originals, AI response fixtures, XSS, desktop/mobile and keyboard.');
} catch (error) {
  if (diagnosticPage) {
    await diagnosticPage.screenshot({ path: 'build/webgui/screenshots/failure.png', fullPage: true });
    console.error(await diagnosticPage.locator('main').innerText());
    console.error(await diagnosticPage.locator('[role="alert"]').allTextContents());
    console.error(networkFailures);
  }
  throw error;
} finally { await browser.close(); }
