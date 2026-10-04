import { Client, dateScope, safeLink, sessionDelay } from './client.mjs';
import { el, button, badge, dated, numeric, quantity, originalReference, detail, table, chart } from './presentation.mjs';

const root = document.querySelector('#app');
const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone;
let controller = new AbortController();
let generation = 0;
let expiryTimer;
let metrics = [];
let capabilities = {};
let main;
let notice;
let closePreview = () => {};
const filters = { metric: 'ldl_cholesterol', second: '', kind: 'resting_heart_rate' };
const client = new Client(window.fetch.bind(window), () => showLogin('Your session has expired. Sign in again.'));

function resetView() {
  controller.abort();
  controller = new AbortController();
  generation += 1;
  closePreview();
}
function message(text, failed) {
  if (!notice) return;
  notice.textContent = text;
  notice.className = failed ? 'notice error' : 'notice';
  notice.hidden = !text;
}
function empty(text) { return el('p', text, 'empty'); }
function section(title) {
  const node = el('section', null, 'panel');
  node.append(el('h2', title));
  return node;
}
function field(label, input) {
  const wrap = el('label', null, 'field');
  wrap.append(el('span', label), input);
  return wrap;
}
function select(options, selected) {
  const node = el('select');
  for (const [value, name] of options) {
    const option = el('option', name);
    option.value = value;
    node.append(option);
  }
  node.value = selected;
  return node;
}
function route(view, id) {
  location.hash = id ? `${view}/${encodeURIComponent(id)}` : view;
}
function reportLink(id, text) {
  const link = el('a', text, 'text-link');
  link.href = `#reports/${encodeURIComponent(id)}`;
  return link;
}
async function action(node, work) {
  const current = generation;
  node.disabled = true;
  message('', false);
  try { await work(); }
  catch (error) { if (current === generation && error.name !== 'AbortError') message(error.message, true); }
  finally { node.disabled = false; }
}
function actionButton(text, work, className) {
  const node = button(text, () => action(node, work), className);
  return node;
}
function post(path, body) { return client.post(path, body, controller.signal); }

function scheduleExpiry() {
  if (!client.session) return;
  if (client.session.expires_at * 1000 <= Date.now()) {
    showLogin('Your session has expired. Sign in again.');
    return;
  }
  expiryTimer = setTimeout(scheduleExpiry, sessionDelay(client.session.expires_at, Date.now()));
}

function showLogin(reason) {
  resetView();
  clearTimeout(expiryTimer);
  client.clear();
  metrics = [];
  capabilities = {};
  root.replaceChildren();
  const page = el('main', null, 'login-page');
  page.id = 'content';
  const intro = el('section', null, 'login-intro');
  intro.append(el('div', 'h+', 'brand-mark'), el('p', 'helpyourself', 'wordmark'),
    el('h1', 'Your health.\nYour history.'),
    el('p', 'A clear view of your reports, your progress, and the questions worth asking.', 'intro-copy'));
  const form = el('form', null, 'login-form panel');
  form.append(el('p', 'YOUR PERSONAL ARCHIVE', 'eyebrow'), el('h2', 'Welcome back'),
    el('p', 'Sign in to the helpyourself server hosting this page.', 'muted'));
  const username = el('input');
  username.name = 'username'; username.autocomplete = 'username'; username.required = true;
  username.maxLength = 128; username.autocapitalize = 'none'; username.spellcheck = false;
  const password = el('input');
  password.type = 'password'; password.name = 'password'; password.autocomplete = 'current-password'; password.required = true;
  form.append(field('Username', username), field('Password', password));
  notice = el('p', '', 'notice'); notice.role = 'alert'; notice.hidden = true;
  const submit = el('button', 'Sign in', 'primary'); submit.type = 'submit';
  form.append(notice, submit, el('p', 'Accounts are created by your server administrator. Import reports and connect health platforms using the mobile app.', 'footnote'));
  form.addEventListener('submit', event => {
    event.preventDefault();
    action(submit, async () => {
      const signal = controller.signal;
      const session = await client.post('session/login', { username: username.value, password: password.value }, signal);
      password.value = '';
      if (signal.aborted) return;
      client.session = session;
      try {
        const [status, catalog] = await Promise.all([
          client.post('server/status', {}, signal), client.post('metrics/list', {}, signal),
        ]);
        if (signal.aborted) return;
        capabilities = status.capabilities;
        metrics = catalog.metrics;
        scheduleExpiry();
        showShell();
      } catch (error) {
        client.clear();
        throw error;
      }
    });
  });
  page.append(intro, form); root.append(page);
  message(reason, Boolean(reason));
}

function showShell() {
  root.replaceChildren();
  const shell = el('div', null, 'shell');
  const sidebar = el('aside', null, 'sidebar');
  const brand = el('a', null, 'brand'); brand.href = '#reports';
  brand.append(el('span', 'h+', 'brand-mark'), el('span', 'helpyourself', 'wordmark'));
  const nav = el('nav'); nav.setAttribute('aria-label', 'Main navigation');
  const items = [['reports', 'Reports', '01'], ['trends', 'Trends', '02'], ['health', 'Health', '03'], ['insights', 'Insights', '04'], ['settings', 'Settings', '05']];
  for (const [name, title, index] of items) {
    const link = el('a'); link.href = `#${name}`; link.dataset.view = name;
    link.append(el('span', index, 'nav-number'), el('span', title)); nav.append(link);
  }
  const bottom = el('div', null, 'sidebar-bottom');
  bottom.append(el('p', 'YOUR SERVER · YOUR DATA', 'eyebrow'), el('p', client.session.user.username, 'account-name'));
  bottom.append(actionButton('Sign out', async () => {
    let failure;
    try { await post('session/logout', {}); }
    catch (error) { failure = error; }
    showLogin(failure ? 'Signed out of this page. Server session revocation failed; it will expire automatically.' : '');
  }, 'quiet'));
  sidebar.append(brand, nav, bottom);
  const workspace = el('div', null, 'workspace');
  const topbar = el('header', null, 'topbar');
  topbar.append(el('span', 'Your health archive', 'muted'), el('span', 'Web viewer', 'viewer-tag'));
  notice = el('p', '', 'notice'); notice.role = 'alert'; notice.hidden = true;
  main = el('main'); main.id = 'content'; main.tabIndex = -1;
  workspace.append(topbar, notice, main); shell.append(sidebar, workspace); root.append(shell);
  navigate();
}

const titles = {
  reports: ['Reports', 'The original evidence, always within reach.'],
  trends: ['Trends', 'See how confirmed results change over time.'],
  health: ['Health', 'The details your mobile devices have synced.'],
  insights: ['Insights', 'A focused review, grounded in your archived data.'],
  settings: ['Settings', 'Your connection and available services.'],
};
async function navigate() {
  if (!client.session) return;
  resetView();
  const current = generation;
  const signal = controller.signal;
  const [requested, encoded] = location.hash.slice(1).split('/');
  const view = titles[requested] ? requested : 'reports';
  let id;
  try { id = encoded ? decodeURIComponent(encoded) : null; }
  catch { id = null; }
  for (const link of root.querySelectorAll('nav a')) {
    if (link.dataset.view === view) link.setAttribute('aria-current', 'page');
    else link.removeAttribute('aria-current');
  }
  message('', false);
  main.replaceChildren(el('p', 'YOUR HEALTH, IN CONTEXT', 'eyebrow'), el('h1', titles[view][0]), el('p', titles[view][1], 'subtitle'));
  const loading = el('p', 'Loading your archive…', 'empty'); loading.role = 'status';
  main.append(loading); main.setAttribute('aria-busy', 'true');
  try {
    const render = {
      reports: () => id ? reportView(id) : reportsView(), trends: trendsView,
      health: healthView, insights: () => id ? analysisView(id) : insightsView(), settings: settingsView,
    }[view];
    const content = await render();
    if (current !== generation) return;
    loading.replaceWith(content);
  } catch (error) {
    if (current !== generation || signal.aborted) return;
    loading.replaceWith(empty('This view could not be loaded.'));
    message(error.message, true);
    main.append(actionButton('Try again', navigate, 'primary'));
  } finally { if (current === generation) main.removeAttribute('aria-busy'); }
}

async function reportsView() {
  const wrap = el('div');
  const panel = section('Report library');
  panel.append(button('Refresh reports', navigate, 'quiet'));
  panel.append(el('p', 'Reports are imported and reviewed in the mobile app. Pending results remain visible here with their review status.', 'muted'));
  const list = el('div', null, 'report-list');
  let afterID = null;
  const load = async () => {
    const signal = controller.signal;
    const data = await post('reports/list', { limit: 100, after_id: afterID });
    if (signal.aborted) return;
    if (!data.reports.length && !afterID) list.append(empty('No reports yet. Import your first report from the mobile app.'));
    for (const report of data.reports) {
      const link = reportLink(report.report_id, ''); link.className = 'report-row';
      const info = el('div');
      info.append(el('strong', report.original_name), el('span', `${dated(report.created_at)} · ${report.page_count} pages · revision ${report.revision}`, 'muted'));
      link.append(el('span', '↗', 'report-icon'), info, el('span', 'View report →', 'row-action'));
      list.append(link);
    }
    afterID = data.reports.at(-1)?.report_id ?? afterID;
    more.hidden = data.reports.length < 100;
  };
  const more = actionButton('Load more reports', load, 'quiet');
  panel.append(list, more); wrap.append(panel);
  await load(); return wrap;
}

async function preview(reportID, page, name) {
  const signal = controller.signal;
  const blob = await client.original(reportID, signal);
  if (signal.aborted) return;
  closePreview();
  const url = URL.createObjectURL(blob);
  const dialog = el('dialog', null, 'preview');
  const head = el('div', null, 'preview-header');
  head.append(el('h2', name), el('span', `Source page ${page}`, 'muted'));
  const close = button('Close', () => dialog.close(), 'quiet');
  const download = el('a', 'Download original', 'button quiet'); download.href = url; download.download = name;
  head.append(download, close);
  let viewer;
  if (blob.type === 'application/pdf') {
    viewer = el('iframe'); viewer.title = `Original report: ${name}`; viewer.src = `${url}#page=${page}`;
  } else if (['image/png', 'image/jpeg'].includes(blob.type)) {
    viewer = el('img'); viewer.src = url; viewer.alt = `Original report: ${name}`;
  } else { viewer = empty('This browser may not display HEIC/HEIF. Download the preserved original to view it on your device.'); }
  dialog.append(head, viewer);
  dialog.addEventListener('close', () => { URL.revokeObjectURL(url); dialog.remove(); closePreview = () => {}; });
  closePreview = () => dialog.close();
  root.append(dialog); dialog.showModal(); close.focus();
}

async function reportView(id) {
  const data = await post('reports/get', { report_id: id });
  const wrap = el('div');
  const panel = section(data.report.original_name);
  const toolbar = el('div', null, 'toolbar');
  toolbar.append(button('← All reports', () => route('reports'), 'quiet'),
    actionButton('Open original', () => preview(id, 1, data.report.original_name), 'primary'));
  panel.append(toolbar, el('p', `${data.report.page_count} pages · revision ${data.report.revision} · archived ${dated(data.report.created_at)}`, 'muted'));
  if (data.relation) panel.append(el('p', `Report relationship: ${data.relation.kind}`, 'muted'), reportLink(data.relation.preferred_report_id, 'View preferred report'));
  if (!data.observations.length) panel.append(empty('No extracted or reviewed results yet. Use the mobile app to complete review.'));
  for (const observation of data.observations) {
    const payload = observation.payload;
    const item = el('article', null, 'observation');
    const heading = el('div', null, 'observation-heading');
    heading.append(el('h3', payload.raw_name), badge(observation.status));
    item.append(heading, el('p', `${payload.raw_result} ${payload.raw_unit ?? ''}`, 'result-value'),
      el('p', `Standardized: ${quantity(observation.interpretation.result)}`),
      el('p', `Printed reference: ${originalReference(payload, observation.interpretation.reference)}`, 'muted'),
      el('p', `Standardized reference: ${quantity(observation.interpretation.reference)}`, 'muted'),
      el('p', `Collected: ${payload.sampled_at ?? 'Unknown'} · report flag: ${payload.report_flag ?? 'None'} · revision ${observation.revision}`, 'muted'));
    if (payload.notes) item.append(el('p', payload.notes));
    item.append(el('blockquote', payload.source.quote),
      actionButton(`Open source · page ${payload.source.page}`, () => preview(id, payload.source.page, data.report.original_name), 'quiet'));
    const history = actionButton('Show revision history', async () => {
      const signal = controller.signal;
      const result = await post('observations/history', { observation_id: observation.observation_id });
      if (!signal.aborted) { history.replaceWith(detail('Observation revisions', result.history)); }
    }, 'quiet');
    item.append(history, detail('Structured result and provenance', observation)); panel.append(item);
  }
  if (data.context) panel.append(detail('Report context', data.context));
  for (const page of data.pages) panel.append(detail(`Recognized text · page ${page.page} · ${page.status}`, page.content));
  for (const input of data.extraction_inputs) {
    panel.append(actionButton(`View input evidence · page ${input.page}`, async () => {
      const signal = controller.signal;
      const evidence = await post('reports/input/get', { report_id: id, run_id: input.run_id, page: input.page });
      if (!signal.aborted) panel.append(detail('Input evidence', evidence));
    }, 'quiet'));
  }
  for (const output of data.extraction_outputs) {
    panel.append(actionButton(`View ${output.stage} reply · page ${output.page}`, async () => {
      const signal = controller.signal;
      const reply = await post('reports/extraction/get', { report_id: id, run_id: output.run_id, page: output.page, stage: output.stage });
      if (!signal.aborted) panel.append(detail(`${output.stage} reply`, reply));
    }, 'quiet'));
  }
  wrap.append(panel); return wrap;
}

async function trendsView() {
  const wrap = el('div');
  const controls = section('Compare your history'); controls.classList.add('controls');
  const options = metrics.map(metric => [metric.metric_id, metric.name]);
  const primary = select(options, filters.metric);
  const secondary = select([['', 'Single metric'], ...options], filters.second);
  controls.append(field('Metric', primary), field('Compare with', secondary));
  controls.append(button('Refresh history', navigate, 'quiet'));
  for (const input of [primary, secondary]) input.addEventListener('change', () => {
    filters.metric = primary.value; filters.second = secondary.value; navigate();
  });
  wrap.append(controls);
  const ids = [...new Set([filters.metric, filters.second].filter(Boolean))];
  const results = await Promise.all(ids.map(async metric => ({ metric, data: await post('trends/get', { metric_ids: [metric], start_at: null, end_at: null }) })));
  for (const { metric, data } of results) {
    const title = metrics.find(item => item.metric_id === metric)?.name ?? metric;
    const panel = section(title);
    if (!data.points.length) panel.append(empty('No comparable results. Confirm a mapped metric, recognized unit and collection date in the mobile app.'));
    else {
      const unit = data.points[0].unit;
      panel.append(el('p', `${unit} · ${data.points.length} confirmed results · all years`, 'muted'));
      const open = point => route('reports', point.report_id);
      panel.append(chart(data.points, `${title} in ${unit}`, open));
      panel.append(table(['Collected', 'Standardized', 'Printed result', 'Reference', 'Source'], data.points.map(point => [
        point.sampled_at, `${point.value} ${point.unit}`, `${point.original.raw_result} ${point.original.raw_unit ?? ''}`,
        quantity(point.reference), reportLink(point.report_id, `Report · page ${point.original.source.page}`),
      ])));
    }
    if (data.incomparable_count) panel.append(el('p', `${data.incomparable_count} confirmed results cannot be compared due to date, unit or value limitations. They remain in Reports.`, 'footnote'));
    wrap.append(panel);
  }
  return wrap;
}

async function healthView() {
  const wrap = el('div');
  const controls = section('Last 30 days'); controls.classList.add('controls');
  const input = select([
    ['resting_heart_rate', 'Resting heart rate'], ['heart_rate', 'Heart rate'], ['hrv_sdnn', 'HRV SDNN'],
    ['steps', 'Steps'], ['sleep', 'Sleep'], ['workout', 'Workout'],
  ], filters.kind);
  input.addEventListener('change', () => { filters.kind = input.value; navigate(); });
  controls.append(field('Metric', input), el('p', timezone, 'muted'));
  controls.append(button('Refresh data', navigate, 'quiet'));
  wrap.append(controls);
  const [coverage, aggregate, raw] = await Promise.all([
    post('health/coverage', {}),
    post('health/aggregate', { record_type: filters.kind, ...dateScope(30, new Date(), timezone) }),
    post('health/list', { limit: 50, offset: 0 }),
  ]);
  const daily = section('Daily view');
  daily.append(el('p', 'Sources remain separate. Missing days are not zero; sample averages are not daily clinical measurements.', 'muted'));
  if (!aggregate.days.length) daily.append(empty('No visible samples for this metric. Sync from your mobile device to add data.'));
  else daily.append(table(['Date', 'Value', 'Source', 'Samples', 'Method'], aggregate.days.map(day => [
    day.date, day.value === null ? 'No visible samples' : `${numeric(day.value)} ${day.unit ?? ''}`, day.source, day.sample_count, day.algorithm,
  ])));
  const sync = section('Sync coverage');
  sync.append(el('p', 'Coverage describes data visible to the syncing device. Empty queries do not establish whether permission was denied or data is absent.', 'muted'));
  if (!coverage.coverage.length) sync.append(empty('No mobile connections yet. Connect Apple Health in the iPhone app.'));
  else sync.append(table(['Platform', 'Type', 'Status', 'Last successful query'], coverage.coverage.map(entry => [
    entry.platform, entry.record_type, badge(entry.status), dated(entry.last_success_at),
  ])));
  const records = section('Archived details');
  records.append(el('p', 'Browse the original structured records, including platform metadata and nested series.', 'muted'));
  const list = el('div');
  const append = values => {
    for (const entry of values) {
      const record = entry.record;
      list.append(detail(`${entry.platform} · ${record.record_type} · ${dated(record.start_at)}`, record));
    }
  };
  append(raw.records);
  if (!raw.records.length) list.append(empty('No health records archived yet.'));
  let offset = raw.records.length;
  const more = actionButton('Load more records', async () => {
    const signal = controller.signal;
    const next = await post('health/list', { limit: 50, offset });
    if (signal.aborted) return;
    offset += next.records.length; append(next.records); more.hidden = next.records.length < 50;
  }, 'quiet');
  more.hidden = raw.records.length < 50;
  records.append(list, more); wrap.append(daily, sync, records); return wrap;
}

async function insightsView() {
  const data = await post('analysis/list', {});
  const wrap = el('div');
  const preset = section('Lipid history & recent health');
  preset.append(el('p', 'PERSONAL RESEARCH PREVIEW', 'eyebrow'),
    el('p', 'Review confirmed lipid results with the last 90 calendar days of synced health data. The server uses its configured AI service.'),
    el('p', 'AI hypotheses are unverified. Check the cited data and use the suggested questions when speaking with your clinician.', 'footnote'));
  const create = actionButton('Run lipid review', async () => {
    const signal = controller.signal;
    const result = await post('analysis/create', dateScope(90, new Date(), timezone));
    if (!signal.aborted) route('insights', result.run_id);
  }, 'primary');
  create.disabled = !capabilities.analysis;
  preset.append(create);
  if (!capabilities.analysis) preset.append(el('p', 'AI analysis is disabled on this server. Your administrator can configure and enable the analysis provider.', 'muted'));
  const runs = section('Your reviews');
  runs.append(button('Refresh reviews', navigate, 'quiet'));
  if (!data.runs.length) runs.append(empty('No reviews yet. Run the preset when confirmed lipid data is available.'));
  for (const run of data.runs) {
    const link = el('a', null, 'report-row'); link.href = `#insights/${encodeURIComponent(run.run_id)}`;
    const info = el('div'); info.append(el('strong', 'Lipid review'), el('span', dated(run.created_at), 'muted'));
    link.append(info, badge(run.status), el('span', 'View →', 'row-action')); runs.append(link);
  }
  wrap.append(preset, runs); return wrap;
}

async function analysisView(id) {
  const run = await post('analysis/get', { run_id: id });
  const wrap = el('div');
  const panel = section('Lipid review');
  const toolbar = el('div', null, 'toolbar');
  toolbar.append(button('← All reviews', () => route('insights'), 'quiet'), button('Refresh status', navigate, 'quiet'), badge(run.status));
  panel.append(toolbar, el('p', 'Unverified · personal research preview', 'footnote'));
  if (run.status === 'stale') panel.append(el('p', 'Source data changed. This review has been invalidated. Return to Insights to create a new review.', 'notice error'));
  if (run.status === 'failed') {
    const retry = actionButton('Retry review', async () => { await post('analysis/retry', { run_id: id }); await navigate(); }, 'primary');
    retry.disabled = !capabilities.analysis; panel.append(el('p', 'The review failed.'), retry);
  }
  if (['queued', 'running'].includes(run.status)) panel.append(el('p', 'The server is processing this review. Refresh to check its status.', 'notice'));
  if (run.output?.review) {
    panel.append(el('p', run.output.review.summary, 'analysis-summary'));
    for (const finding of run.output.review.findings) {
      const item = el('article', null, 'observation');
      item.append(el('h3', finding.title), el('p', finding.hypothesis));
      for (const [title, values] of [
        ['Other explanations', finding.other_explanations], ['Missing information', finding.missing_information], ['Questions for your clinician', finding.questions_for_clinician],
      ]) {
        item.append(el('h4', title)); const list = el('ul');
        for (const value of values) list.append(el('li', value)); item.append(list);
      }
      item.append(el('h4', 'Source reports'));
      for (const observationID of finding.observation_ids) {
        const source = run.input.observations.find(observation => observation.observation_id === observationID);
        if (source) item.append(reportLink(source.report_id, `${source.payload.raw_name}: ${source.payload.raw_result} ${source.payload.raw_unit ?? ''}`));
      }
      panel.append(item);
    }
    const references = section('References');
    for (const source of run.output.evidence ?? []) {
      const href = safeLink(source.url);
      if (!href) { references.append(el('p', source.title)); continue; }
      const link = el('a', source.title, 'text-link'); link.href = href; link.target = '_blank'; link.rel = 'noopener noreferrer'; references.append(link);
    }
    panel.append(references);
  }
  panel.append(detail('Review scope and source snapshot', run.input));
  wrap.append(panel); return wrap;
}

function settingsView() {
  const panel = section('Your connection');
  panel.append(table(['Setting', 'Value'], [
    ['Server', location.origin], ['Account', client.session.user.username], ['Session expires', dated(client.session.expires_at)],
    ['Browser timezone', timezone], ['AI analysis', capabilities.analysis ? 'Enabled' : 'Disabled'],
  ]));
  panel.append(el('h3', 'Mobile & web'),
    el('p', 'Use the mobile app to import reports, review results, sync platform data to this server, and write supported confirmed results to Apple Health.'),
    el('p', 'The web viewer browses this archive and runs the server’s preset analysis. AI service configuration belongs to your server.'),
    el('p', 'Your sign-in token stays in this page’s memory. Reloading or closing the page requires signing in again.', 'muted'));
  return panel;
}

window.addEventListener('hashchange', navigate);
window.matchMedia('(max-width: 700px)').addEventListener('change', () => { if (client.session) navigate(); });
showLogin('');
