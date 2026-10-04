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
    await page.getByRole('navigation').getByRole('link', { name: /Reports/ }).click();
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
  await nav('Health');
  await page.getByRole('heading', { name: 'Archived details' }).waitFor();
  assert.ok(await page.getByText('apple_health:synthetic-watch', { exact: true }).first().isVisible());
  await page.getByText(/apple_health · resting_heart_rate/).click();
  assert.ok(await page.getByText(/"record_id": "browser-sample"/).isVisible());
  await screenshot('health-desktop.png');
  await nav('Insights');
  assert.ok(await page.getByRole('button', { name: 'Run lipid review' }).isDisabled());
  await nav('Reports');
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
