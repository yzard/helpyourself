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
let selectedDate = dateScope(1, new Date(), timezone).end_date;
let sourcePriority = {};
let importCursor = null;
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
  input.setAttribute('aria-label', label);
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
  sourcePriority = {};
  importCursor = null;
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
  const brand = el('a', null, 'brand'); brand.href = '#overview';
  brand.append(el('span', 'h+', 'brand-mark'), el('span', 'helpyourself', 'wordmark'));
  const nav = el('nav'); nav.setAttribute('aria-label', 'Main navigation');
  const items = [['overview', 'Overview', '01'], ['reports', 'Archive', '02'], ['trends', 'Trends', '03'], ['insights', 'Insights', '04'], ['data', 'Data', '05']];
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
  topbar.append(el('span', 'Your health archive', 'muted'), button('Settings', () => route('settings'), 'quiet'));
  notice = el('p', '', 'notice'); notice.role = 'alert'; notice.hidden = true;
  main = el('main'); main.id = 'content'; main.tabIndex = -1;
  workspace.append(topbar, notice, main); shell.append(sidebar, workspace); root.append(shell);
  navigate();
}

const titles = {
  associations: ["Behavior associations", "Declared comparisons with uncertainty and missingness."],
  breathing: ['Breathing practice', 'A self-paced practice with a five-minute timer.'],
  review: ['Log review', 'Recorded training, nutrition and body measurements.'],
  timeline: ['Timeline', 'Archived events, with their source and revision.'],
  series: ['Device trends', 'Measurements remain separate for each source.'],
  research: ['Research model', 'A fixed blood-test model with explicit input evidence.'],
  sleep: ['Sleep', 'Observed sessions and source stage intervals.'],
  entries: ['Daily logs', 'Your own observations, meals and training.'],
  overview: ['Overview', 'Your daily records, with their sources and coverage.'],
  data: ['Data', 'Manage sources and take your archive with you.'],
  reports: ['Archive', 'The original evidence, always within reach.'],
  trends: ['Trends', 'See how confirmed results change over time.'],
  health: ['Health', 'The details your mobile devices have synced.'],
  hrv: ['HRV windows', 'Quality-reviewed intervals and separate protocol baselines.'],
  insights: ['Insights', 'A focused review, grounded in your archived data.'],
  settings: ['Settings', 'Your connection and available services.'],
};
async function navigate() {
  if (!client.session) return;
  resetView();
  const current = generation;
  const signal = controller.signal;
  const [requested, encoded] = location.hash.slice(1).split('/');
  const view = titles[requested] ? requested : 'overview';
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
      hrv: hrvView, associations: associationsView, breathing: breathingView, review: reviewView, timeline: timelineView, series: seriesView, overview: overviewView, data: dataView, entries: entriesView, sleep: sleepView, research: researchView,
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
  panel.append(button('Refresh reports', navigate, 'quiet'), button('Device records', () => route('health'), 'quiet'));
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
  wrap.append(button('Device trends', () => route('series'), 'quiet'), button('Review daily logs', () => route('review'), 'quiet'));
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
  wrap.append(button('HRV windows', () => route('hrv'), 'quiet'), button('Behavior associations', () => route('associations'), 'quiet'));
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
    const info = el('div'); info.append(el('strong', run.prompt_version === 'archive-coach-v1' ? 'Archive question' : 'Lipid review'), el('span', dated(run.created_at), 'muted'));
    link.append(info, badge(run.status), el('span', 'View →', 'row-action')); runs.append(link);
  }
  wrap.append(preset, runs, button('Blood-test research model', () => route('research'), 'quiet')); return wrap;
}

async function analysisView(id) {
  const run = await post('analysis/get', { run_id: id });
  const wrap = el('div');
  const panel = section(run.input?.prompt_version === 'archive-coach-v1' ? 'Archive question' : 'Lipid review');
  const toolbar = el('div', null, 'toolbar');
  toolbar.append(button('← All reviews', () => route('insights'), 'quiet'), button('Refresh status', navigate, 'quiet'), badge(run.status));
  panel.append(toolbar, el('p', 'Unverified · personal research preview', 'footnote'));
  if (run.status === 'stale') panel.append(el('p', 'Source data changed. This review has been invalidated. Return to Insights to create a new review.', 'notice error'));
  if (run.status === 'failed') {
    const retry = actionButton('Retry review', async () => { await post('analysis/retry', { run_id: id }); await navigate(); }, 'primary');
    retry.disabled = !capabilities.analysis; panel.append(el('p', 'The review failed.'), retry);
  }
  if (['queued', 'running'].includes(run.status)) panel.append(el('p', 'The server is processing this review. Refresh to check its status.', 'notice'));
  if (run.output?.coach) {
    panel.append(el('p', run.input.question));
    for (const claim of run.output.coach.claims) { const item = section('Unverified archive claim'); item.append(el('p', claim.text)); for (const id of claim.evidence_ids) item.append(detail(id, run.input.evidence.find(source => source.id === id))); panel.append(item); }
    for (const missing of run.output.coach.missing_information) panel.append(el('p', missing, 'notice'));
    for (const draft of run.output.coach.drafts) panel.append(detail(`Unsubmitted draft: ${draft.title}`, draft.entry));
  }
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

const metricNames = { sleep: 'Sleep', workout: 'Workouts', steps: 'Steps', resting_heart_rate: 'Resting heart rate', heart_rate: 'Heart rate', hrv_sdnn: 'HRV SDNN' };
async function overviewView() {
  const wrap = el('div');
  wrap.append(button('Open timeline', () => route('timeline'), 'quiet'));
  const controls = section('Daily archive'); controls.classList.add('controls');
  const date = el('input'); date.type = 'date'; date.value = selectedDate;
  date.addEventListener('change', () => { if (date.value) { selectedDate = date.value; navigate(); } });
  controls.append(field('Date', date), el('p', timezone, 'muted'), button('Refresh', navigate, 'quiet'));
  wrap.append(controls);
  const saved = await post('wellness/preferences/get', {});
  sourcePriority = saved.preferences.source_priority;
  const snapshot = await post('wellness/day', { date: selectedDate, timezone, source_priority: sourcePriority });
  wrap.append(el('p', `Calculated ${dated(snapshot.computed_at)}`, 'muted'));
  const customize = el('details'); customize.append(el('summary', 'Customize overview'));
  const favorites = new Map();
  for (const metric of snapshot.metrics) {
    const checkbox = el('input'); checkbox.type = 'checkbox'; checkbox.checked = saved.preferences.favorite_metrics.includes(metric.record_type);
    favorites.set(metric.record_type, checkbox); customize.append(field(metricNames[metric.record_type], checkbox));
  }
  const target = el('input'); target.type = 'number'; target.min = '1'; target.max = '1440'; target.value = saved.preferences.sleep_target_minutes ?? '';
  customize.append(field('Your daily sleep target (minutes, optional)', target), el('p', 'Select favorite metrics to show. No selection shows all metrics. A target is your own goal, not a calculated biological sleep need.', 'footnote'));
  customize.append(actionButton('Save overview preferences', async () => {
    await post('wellness/preferences/save', { expected_version: saved.version, batch_id: crypto.randomUUID(), preferences: { ...saved.preferences, favorite_metrics: [...favorites].filter(([, input]) => input.checked).map(([kind]) => kind), sleep_target_minutes: target.value ? Number(target.value) : null } }); await navigate();
  }, 'quiet'));
  controls.append(customize);
  const grid = el('div', null, 'metric-grid');
  for (const metric of snapshot.metrics.filter(m => !saved.preferences.favorite_metrics.length || saved.preferences.favorite_metrics.includes(m.record_type))) {
    const card = section(metricNames[metric.record_type] ?? metric.record_type);
    const selected = metric.current.selected;
    card.append(el('p', selected ? `${numeric(selected.value)} ${selected.unit ?? ''}` : 'Not available', 'metric-value'));
    card.append(badge(metric.current.state), el('p', selected?.source ?? (metric.current.state === 'source_selection_required' ? 'Choose the source to use for this metric.' : metric.current.state === 'query_limit_exceeded' ? 'This history exceeds the query limit. Open device records for a shorter range.' : 'No usable records for this day.'), 'muted'));
    if (metric.record_type === 'sleep' && selected && saved.preferences.sleep_target_minutes) card.append(el('p', `${numeric(selected.value / 60 - saved.preferences.sleep_target_minutes)} minutes relative to your daily target`, 'footnote'));
    if (metric.record_type === 'sleep') card.append(el('p', 'Calendar-day sleep total, including naps. This is not a nightly recovery score.', 'footnote'));
    const sources = [...new Set(metric.history.flatMap(day => day.alternatives.map(item => item.source)))];
    if (sources.length) {
      const choice = select([['', 'Use only when one source is available'], ...sources.map(s => [s, s])], sourcePriority[metric.record_type]?.[0] ?? '');
      choice.addEventListener('change', () => action(choice, async () => {
        const priorities = { ...sourcePriority };
        if (choice.value) priorities[metric.record_type] = [choice.value];
        else delete priorities[metric.record_type];
        await post('wellness/preferences/save', { expected_version: saved.version, batch_id: crypto.randomUUID(), preferences: { ...saved.preferences, source_priority: priorities } });
        await navigate();
      }));
      card.append(field(`Source for ${metricNames[metric.record_type]}`, choice));
    }
    card.append(el('p', metric.baseline ? `Previous 28 valid days: median ${numeric(metric.baseline.median)} ${selected?.unit ?? ''}` : `Personal baseline: ${metric.baseline_valid_days}/28 valid days`, 'footnote'));
    const spark = dailySparkline(metric); if (spark) card.append(spark);
    const history = el('details'); history.append(el('summary', 'History and source comparison'));
    history.append(table(['Date', 'Source', 'Value', 'State'], metric.history.flatMap(day => day.alternatives.map(item => [day.date, item.source, `${numeric(item.value)} ${item.unit ?? ''}`, day.state]))));
    history.append(detail('Calculation evidence', { version: metric.algorithm_version, archive_revision: snapshot.data_revision, source_policy: snapshot.source_policy, baseline: metric.baseline, evidence_status: snapshot.evidence_status }));
    card.append(history); grid.append(card);
  }
  const actions = el('div', null, 'toolbar');
  actions.append(button('Breathing practice', () => route('breathing'), 'quiet'));
  actions.append(button('Record a daily log', () => route('entries'), 'primary'), button('Sleep sessions', () => route('sleep'), 'quiet')); wrap.append(actions);
  wrap.append(grid, button('Browse device records', () => route('health'), 'quiet'));
  return wrap;
}

async function dataView() {
  const wrap = el('div');
  const [sources, coverage, exported, imported] = await Promise.all([post('wellness/sources', {}), post('health/coverage', {}), post('exports/list', {}), post('wellness/import/list', { after_id: importCursor })]);
  const panel = section('Data sources');
  panel.append(el('p', 'Visible records do not prove read permission or full device coverage. Connect and sync Apple Health in the iPhone app.', 'muted'));
  panel.append(table(['Platform', 'Source', 'Type', 'Records', 'Latest'], sources.sources.map(s => [s.platform, s.source_id, s.record_type, s.record_count, dated(s.last_at)])));
  panel.append(detail('Sync coverage', coverage.coverage));
  const file = el('input'); file.type = 'file'; file.accept = '.gpx,.tcx,.fit,application/gpx+xml,application/xml';
  panel.append(field('Import a GPX, TCX, or FIT activity', file), actionButton('Import track', async () => {
    const selected = file.files[0];
    if (!selected || selected.size > 4 * 1024 * 1024) throw new Error('Select a GPX, TCX, or FIT file up to 4 MiB.');
    const format = selected.name.toLowerCase().split('.').at(-1);
    if (!['gpx', 'tcx', 'fit'].includes(format)) throw new Error('Use a .gpx, .tcx, or .fit filename.');
    const bytes = new Uint8Array(await selected.arrayBuffer());
    const body = { filename: selected.name };
    if (format === 'fit') {
      let binary = '';
      for (let at = 0; at < bytes.length; at += 8192) binary += String.fromCharCode(...bytes.subarray(at, at + 8192));
      body.data_base64 = btoa(binary);
    } else body.xml = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
    await post(`wellness/import/${format}`, body); await navigate();
  }, 'primary'));
  panel.append(el('p', 'Timed GPX 1.1 tracks, single-activity TCX 2 files, and single-session FIT files are supported. The archive retains original bytes and their SHA-256 digest. FIT timer duration and elapsed duration remain separate.', 'footnote'));
  for (const item of imported.imports) {
    const row = el('div', null, 'observation');
    row.append(el('p', `${item.filename} · ${dated(item.start_at)} · ${item.distance_m == null ? "Distance unknown" : `${numeric(item.distance_m / 1000)} km`}`));
    row.append(actionButton('Delete imported track', async () => {
      if (!window.confirm('Delete this imported track and its original archive? Separate exports and backups remain unchanged.')) return;
      await post('wellness/import/delete', { record_id: item.record_id, source_id: item.source_id, expected_version: item.version }); await navigate();
    }, 'quiet')); panel.append(row);
  }
  if (importCursor) panel.append(button('First imports', () => { importCursor = null; navigate(); }, 'quiet'));
  if (imported.next_after_id) panel.append(button('More imports', () => { importCursor = imported.next_after_id; navigate(); }, 'quiet'));
  wrap.append(panel);
  const exports = section('Export archive');
  exports.append(el('p', 'Includes all current server archive tables, report originals and health revisions. ZIP exports contain private health data. Processing copies are not included.', 'muted'));
  exports.append(actionButton('Create full export', async () => { await post('exports/create', {}); await navigate(); }, 'primary'), button('Refresh exports', navigate, 'quiet'));
  for (const item of exported.exports) {
    const row = el('div', null, 'observation');
    row.append(el('p', dated(item.created_at)), badge(item.status));
    if (item.status === 'ready') row.append(actionButton('Download ZIP', async () => {
      const signal = controller.signal;
      const blob = await client.exportArchive(item.export_id, signal);
      if (signal.aborted) return;
      const url = URL.createObjectURL(blob);
      const link = el('a'); link.href = url; link.download = `helpyourself-${item.export_id}.zip`;
      document.body.append(link); link.click(); link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    }, 'quiet'));
    if (item.status === 'failed') row.append(el('p', 'Export failed. Create another export to retry.', 'notice error'));
    exports.append(row);
  }
  wrap.append(exports); return wrap;
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
window.matchMedia('(max-width: 700px)').addEventListener('change', () => { if (client.session && !main?.querySelector('form')) navigate(); });
showLogin('');

const logForms = {
  exercise: [['name', 'Exercise name', 'required'], ['equipment', 'Equipment', 'required'], ['muscle_groups', 'Muscle groups, one per line', 'lines'], ['instructions', 'Instructions', 'text']],
  workout_template: [['name', 'Template name', 'required'], ['blocks', 'Exercise, equipment, sets, repetitions, seconds, kg, rest seconds; one block per line', 'prescriptions']],
  planned_workout: [['title', 'Workout title', 'required'], ['duration_minutes', 'Planned duration (minutes)', 'required-number'], ['status', 'Plan status', ['planned', 'skipped']], ['blocks', 'Exercise, equipment, sets, repetitions, seconds, kg, rest seconds; one block per line', 'prescriptions']],
  training_day: [["status", "Training day confirmation (selected local date)", ["rest", "all_sessions_logged"]]],
  journal: [['mood', 'Mood (0–10)', 'number'], ['perceived_stress', 'Perceived stress (0–10)', 'number'], ['behaviors', 'Behaviors: one name = yes or no per line', 'behaviors'], ['measurements', 'Measurements: name = number unit, one per line', 'measurements'], ['times', 'Times: name = HH:MM, one per line', 'times']],
  training: [['activity', 'Activity', 'required'], ['duration_minutes', 'Duration (minutes)', 'required-number'], ['ended_at', 'Session ended at', 'required-datetime'], ['paused_minutes', 'Paused minutes (enter 0 if none)', 'required-number'], ['duration_basis', 'Duration includes pauses?', ['elapsed_including_pauses', 'active_excluding_pauses']], ['rpe_cr10', 'Session effort (CR10, 0–10)', 'number'], ['rpe_answered_at', 'Effort answered at (required with CR10)', 'datetime'], ['sets', 'Sets: one exercise, repetitions, kg per line', 'sets']],
  nutrition: [['food', 'Food or drink', 'required'], ['meal', 'Meal', 'required'], ['energy_kcal', 'Energy (kcal)', 'number'], ['protein_g', 'Protein (g)', 'number'], ['carbohydrate_g', 'Carbohydrate (g)', 'number'], ['fat_g', 'Fat (g)', 'number'], ['fiber_g', 'Fiber (g)', 'number'], ['water_ml', 'Water (ml)', 'number'], ['micronutrients', 'Other nutrients: name = value, one per line', 'nutrients']],
  body: [['weight_kg', 'Weight (kg)', 'required-number'], ['body_fat_percent', 'Body fat (%)', 'number'], ['waist_cm', 'Waist (cm)', 'number']],
  blood_pressure: [['systolic_mmhg', 'Systolic (mmHg)', 'required-number'], ['diastolic_mmhg', 'Diastolic (mmHg)', 'required-number'], ['pulse_bpm', 'Pulse (bpm)', 'number'], ['arm', 'Arm', ['unknown', 'left', 'right']], ['posture', 'Posture', ['unknown', 'seated', 'standing', 'supine']]],
  cycle: [['flow', 'Flow', ['none', 'spotting', 'light', 'medium', 'heavy']], ['context', 'Context', ['cycle', 'pregnancy', 'postpartum', 'perimenopause', 'unknown']], ['symptoms', 'Symptoms, one per line', 'lines']],
  breathing: [['duration_minutes', 'Duration (minutes)', 'required-number'], ['breaths_per_minute', 'Breaths per minute (optional)', 'number']],
};
async function entriesView() {
  const wrap = el('div');
  const now = Math.floor(Date.now() / 1000);
  const result = await post('wellness/entries/list', { start_at: now - 30 * 86400, end_at: now + 86400, kind: null });
  const formPanel = section('Record an observation');
  const kind = el('select');
  for (const value of Object.keys(logForms)) { const option = el('option', value.replaceAll('_', ' ')); option.value = value; kind.append(option); }
  formPanel.append(field('Log type', kind));
  const form = el('form'); formPanel.append(form); wrap.append(formPanel);
  let editing = null;
  function draw() {
    form.replaceChildren();
    const at = el('input'); at.type = 'datetime-local'; at.required = true;
    const date = new Date(editing ? editing.at * 1000 : Date.now());
    at.value = new Date(date.getTime() - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
    form.append(field('Observed at', at));
    const inputs = new Map();
    const content = editing?.entry.content ?? {};
    for (const [key, label, type] of [...logForms[kind.value], ['note', 'Notes', 'text']]) {
      const input = el(Array.isArray(type) ? 'select' : ['nutrients', 'prescriptions', 'sets', 'behaviors', 'measurements', 'times', 'lines', 'text'].includes(type) ? 'textarea' : 'input');
      if (Array.isArray(type)) for (const choice of type) { const option = el('option', choice); option.value = choice; input.append(option); }
      else if (type.includes('number')) { input.type = 'number'; input.step = 'any'; input.min = '0'; }
      if (typeof type === 'string' && type.includes('datetime')) input.type = 'datetime-local';
      if (type.startsWith?.('required')) input.required = true;
      if (type === 'nutrients') input.value = Object.entries(content[key] ?? {}).map(([key, value]) => `${key} = ${value}`).join('\n');
      else if (type === 'prescriptions') input.value = (content[key] ?? []).map(block => ['exercise', 'equipment', 'sets', 'repetitions', 'duration_seconds', 'external_weight_kg', 'rest_seconds'].map(key => block[key] ?? '').join(', ')).join('\n');
      else if (type === 'sets') input.value = (content[key] ?? []).map(s => `${s.exercise}, ${s.repetitions}, ${s.external_weight_kg}`).join('\n');
      else if (type === 'behaviors') input.value = Object.entries(content[key] ?? {}).map(([k, v]) => `${k} = ${v ? 'yes' : 'no'}`).join('\n');
      else if (type === 'measurements') input.value = Object.entries(content[key] ?? {}).map(([name, item]) => `${name} = ${item.value} ${item.unit}`).join('\n');
      else if (type === 'times') input.value = Object.entries(content[key] ?? {}).map(([name, minute]) => `${name} = ${String(Math.floor(minute / 60)).padStart(2, '0')}:${String(minute % 60).padStart(2, '0')}`).join('\n');
      else if (type === 'lines') input.value = (content[key] ?? []).join('\n');
      else if (typeof type === 'string' && type.includes('datetime') && content[key] != null) { const date = new Date(content[key] * 1000); input.value = new Date(date.getTime() - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16); }
      else if (content[key] != null) input.value = content[key];
      inputs.set(key, input); form.append(field(label, input));
    }
    const save = el('button', editing ? 'Save revision' : 'Save log', 'primary'); save.type = 'submit'; form.append(save);
    if (editing) form.append(button('Cancel edit', () => { editing = null; kind.disabled = false; draw(); }, 'quiet'));
    const recordID = editing?.record_id ?? crypto.randomUUID();
    const version = (editing?.version ?? 0) + 1;
    form.onsubmit = event => {
      event.preventDefault();
      action(save, async () => {
        const values = {};
        for (const [key, , type] of [...logForms[kind.value], ['note', 'Notes', 'text']]) {
          const text = inputs.get(key).value.trim();
          if (type === 'nutrients') {
            values[key] = Object.create(null);
            for (const line of text ? text.split('\n') : []) {
              const parts = line.split('=').map(part => part.trim());
              if (parts.length !== 2 || !parts[0] || !parts[1] || !Number.isFinite(Number(parts[1])) || Number(parts[1]) < 0 || Object.hasOwn(values[key], parts[0])) throw new Error('Use unique nutrient names with nonnegative numbers.');
              values[key][parts[0]] = Number(parts[1]);
            }
          } else if (type === 'prescriptions') values[key] = text ? text.split('\n').map(line => {
            const parts = line.split(',').map(part => part.trim());
            if (parts.length !== 7) throw new Error('Each prescription needs seven comma-separated fields.');
            const block = {exercise: parts[0], equipment: parts[1]};
            ['sets', 'repetitions', 'duration_seconds', 'external_weight_kg', 'rest_seconds'].forEach((name, index) => { block[name] = parts[index+2] ? Number(parts[index+2]) : null; });
            return block;
          }) : [];
          else if (type === 'sets') values[key] = text ? text.split('\n').map(line => {
            const parts = line.split(',').map(s => s.trim());
            if (parts.length !== 3 || !parts[1] || !parts[2]) throw new Error('Each set needs an exercise, repetitions and kg.');
            return { exercise: parts[0], repetitions: Number(parts[1]), external_weight_kg: Number(parts[2]) };
          }) : [];
          else if (type === 'behaviors') {
            values[key] = Object.create(null);
            for (const line of text ? text.split('\n') : []) {
              const parts = line.split('=').map(s => s.trim());
              if (parts.length !== 2 || !parts[0] || !['yes', 'no'].includes(parts[1]) || Object.hasOwn(values[key], parts[0])) throw new Error('Use a unique behavior name = yes or no on each line.');
              values[key][parts[0]] = parts[1] === 'yes';
            }
          } else if (type === 'measurements' || type === 'times') {
            values[key] = Object.create(null);
            for (const line of text ? text.split('\n') : []) {
              const parts = line.split('=').map(s => s.trim());
              if (parts.length !== 2 || !parts[0] || Object.hasOwn(values[key], parts[0])) throw new Error('Use unique names and one = per line.');
              if (type === 'times') {
                if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(parts[1])) throw new Error('Use a 24-hour time in HH:MM format.');
                const [hour, minute] = parts[1].split(':').map(Number); values[key][parts[0]] = hour * 60 + minute;
              } else {
                const match = /^(\S+)\s+(.+)$/.exec(parts[1]);
                if (!match || !Number.isFinite(Number(match[1]))) throw new Error('Each measurement needs a number and an explicit unit.');
                values[key][parts[0]] = {value: Number(match[1]), unit: match[2]};
              }
            }
          } else if (type === 'lines') values[key] = text ? text.split('\n').map(v => v.trim()).filter(Boolean) : [];
          else if (typeof type === 'string' && type.includes('datetime')) values[key] = text ? Math.floor(new Date(text).getTime()/1000) : null;
          else if (typeof type === 'string' && type.includes('number')) values[key] = text ? Number(text) : null;
          else values[key] = text;
        }
        if (kind.value === 'nutrition') values.origin = content.origin ?? null;
        await post('wellness/entries/save', { record_id: recordID, version, batch_id: crypto.randomUUID(), at: Math.floor(new Date(at.value).getTime() / 1000), timezone, entry: { kind: kind.value, content: values } });
        await navigate();
      });
    };
  }
  kind.onchange = () => { editing = null; draw(); }; draw();
  const list = section('Last 30 days');
  list.append(el('p', 'Empty fields remain unknown. Entries and their revisions are included in your full export.', 'muted'));
  for (const item of result.entries) {
    const card = el('article', null, 'observation');
    card.append(el('h3', `${item.entry.kind.replaceAll('_', ' ')} · ${dated(item.at)}`), detail('Observation and calculation', item));
    if (item.calculation.session_load_au != null) card.append(el('p', `Session load: ${numeric(item.calculation.session_load_au)} AU`));
    card.append(button('Edit', () => { if (item.entry.kind === 'sleep_correction') { route('sleep'); return; } editing = item; kind.value = item.entry.kind; kind.disabled = true; draw(); formPanel.scrollIntoView({ block: 'start' }); }, 'quiet'));
    card.append(actionButton('Delete log', async () => {
      if (!window.confirm('Delete this log and its revision history? Separate exports and backups remain unchanged.')) return;
      await post('wellness/entries/delete', { record_id: item.record_id, version: item.version + 1, batch_id: crypto.randomUUID(), kind: item.entry.kind }); await navigate();
    }, 'quiet'));
    list.append(card);
  }
  if (!result.entries.length) list.append(empty('No entries in this range.'));
  wrap.append(list); return wrap;
}

async function sleepView() {
  const end = Math.floor(Date.now() / 1000);
  const data = await post('wellness/sleep', { start_at: end - 30 * 86400, end_at: end, timezone });
  const wrap = el('div');
  wrap.append(el('p', 'Last 30 days. Sessions use observed records; gaps do not establish wakefulness. Confirm main sleep or naps without replacing the source observations.', 'muted'));
  for (const source of data.sources) {
    const panel = section(source.source);
    for (const session of source.sessions) {
      const card = el('article', null, 'observation');
      card.append(el('h3', dated(session.end_at)), badge(session.state));
      card.append(el('p', session.asleep_seconds == null ? 'Sleep duration unavailable for this session' : `${numeric(session.asleep_seconds / 3600)} hours asleep`));
      card.append(el('p', session.efficiency == null ? 'Sleep efficiency unavailable: complete in-bed evidence is required.' : `${numeric(session.efficiency * 100)}% observed sleep efficiency`));
      card.append(table(['Start', 'End', 'Source stage'], session.timeline.map(t => [dated(t.start_at), dated(t.end_at), ['In bed', 'Asleep, stage unspecified', 'Awake', 'Core', 'Deep', 'REM'][t.category]])));
      card.append(el('p', `Classification: ${session.classification.replaceAll('_', ' ')} · ${session.correction_state.replaceAll('_', ' ')}`));
      if (session.user_asleep_seconds !== null) card.append(el('p', `User estimate: ${numeric(session.user_asleep_seconds / 3600)} hours. Source duration remains unchanged.`));
      const correction = el('details'); correction.append(el('summary', 'Confirm classification or correct your sleep estimate'));
      const existing = session.corrections.length === 1 ? session.corrections[0] : null;
      if (session.corrections.length > 1) correction.append(el('p', 'Delete duplicate corrections in Daily logs before confirming again.'));
      else {
        const form = el('form');
        const classification = select([['unclassified_session', 'Unclassified'], ['main_sleep', 'Main sleep'], ['nap', 'Nap']], session.classification);
        const minutes = el('input'); minutes.type = 'number'; minutes.min = '0'; minutes.max = String((session.end_at - session.start_at) / 60); minutes.step = 'any'; minutes.value = existing?.entry.content.corrected_asleep_minutes ?? '';
        const note = el('textarea'); note.required = true; note.maxLength = 2048; note.value = existing?.entry.content.note ?? '';
        const save = el('button', 'Save sleep confirmation'); save.type = 'submit';
        form.append(field('Classification', classification), field('User estimated sleep minutes (optional)', minutes), field('Reason or context', note), save);
        const recordID = existing?.record_id ?? crypto.randomUUID();
        form.onsubmit = event => { event.preventDefault(); action(save, async () => {
          await post('wellness/entries/save', {record_id: recordID, version: (existing?.version ?? 0) + 1, batch_id: crypto.randomUUID(), at: session.start_at, timezone, entry: {kind: 'sleep_correction', content: {source: source.source, session_start: session.start_at, session_end: session.end_at, classification: classification.value, corrected_asleep_minutes: minutes.value === '' ? null : Number(minutes.value), basis_revisions: Object.fromEntries(session.timeline.map(stage => [stage.record_id, stage.version])), note: note.value}}});
          await navigate();
        }); };
        correction.append(form);
      }
      card.append(correction, detail('Record evidence', session.timeline)); panel.append(card);
    }
    wrap.append(panel);
  }
  if (!data.sources.length) wrap.append(empty('No supported sleep records in this range.'));
  wrap.append(detail('Calculation method', { version: data.algorithm_version, notes: data.notes }));
  const minuteEnd = Math.floor(end / 60) * 60;
  const regularity = await post('wellness/sleep/regularity', {start_at: minuteEnd - 7 * 86400, end_at: minuteEnd, timezone});
  const panel = section('Seven-day sleep regularity');
  panel.append(el('p', 'This requires explicit sleep or wake states for every minute. Missing daytime records remain unknown.', 'footnote'));
  if (!regularity.sources.length) panel.append(empty('No source has sleep/wake intervals in this seven-day window.'));
  for (const source of regularity.sources) panel.append(el('h3', source.source), el('p', source.value === null ? 'Not available' : `${numeric(source.value)} SRI`, 'metric-value'), badge(source.state), el('p', `${source.known_minutes}/${source.total_minutes} known minute states`, 'footnote'));
  panel.append(detail('Regularity inputs and method', regularity)); wrap.append(panel);
  return wrap;
}

async function researchView() {
  const wrap = el('div');
  const reports = await post('reports/list', { limit: 100 });
  const panel = section('Clinical PhenoAge · research');
  panel.append(el('p', 'This is a published statistical model. It is not measured biological age, a diagnosis or an estimate of aging speed. It requires nine exact, reviewed results from the same collection.', 'muted'));
  const form = el('form');
  const report = el('select');
  for (const item of reports.reports) { const option = el('option', item.original_name); option.value = item.report_id; report.append(option); }
  const age = el('input'); age.type = 'number'; age.min = '20'; age.max = '120'; age.step = 'any'; age.required = true;
  const consent = el('input'); consent.type = 'checkbox'; consent.required = true;
  form.append(field('Reviewed report', report), field('Age at collection (years)', age), field('I understand that this is a research result', consent));
  const submit = el('button', 'Calculate from reviewed inputs', 'primary'); submit.type = 'submit'; form.append(submit);
  const output = el('div');
  form.onsubmit = event => { event.preventDefault(); action(submit, async () => {
    const result = await post('wellness/clinical-age', { report_id: report.value, age_at_collection_years: Number(age.value), research_acknowledged: consent.checked });
    output.replaceChildren();
    if (result.value == null) output.append(el('p', `Missing reviewed inputs: ${result.missing_metrics.join(', ')}`));
    else {
      output.append(el('p', `Research model result: ${numeric(result.value)} years`, 'metric-value'), el('p', result.limitations), detail('Collection and calculation evidence', result));
      const link = el('a', 'Published formula correction'); link.href = result.reference; link.target = '_blank'; link.rel = 'noopener noreferrer'; output.append(link);
    }
  }); };
  panel.append(form, output); wrap.append(panel); return wrap;
}

function dailySparkline(metric) {
  const points = metric.history.filter(day => day.selected && Number.isFinite(day.selected.value));
  if (!points.length) return null;
  const figure = el('figure', null, 'sparkline');
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 280 70'); svg.setAttribute('role', 'img');
  svg.setAttribute('aria-label', `${metricNames[metric.record_type]}: ${points.length} recorded days. Daily values appear in the history table.`);
  const values = points.map(p => p.selected.value), low = Math.min(...values), high = Math.max(...values);
  const times = points.map(p => Date.parse(`${p.date}T12:00:00Z`));
  const x = i => times.length === 1 || times.at(-1) === times[0] ? 140 : 8 + 264 * (times[i] - times[0]) / (times.at(-1) - times[0]);
  const y = i => high === low ? 35 : 62 - 54 * (values[i] - low) / (high - low);
  for (let i = 0; i < points.length; i++) {
    if (i && times[i] - times[i - 1] === 86400000 && points[i].selected.source === points[i - 1].selected.source) {
      const line = document.createElementNS(svg.namespaceURI, 'line');
      for (const [key, value] of Object.entries({ x1: x(i - 1), y1: y(i - 1), x2: x(i), y2: y(i) })) line.setAttribute(key, value);
      line.setAttribute('class', 'chart-line'); svg.append(line);
    }
    const circle = document.createElementNS(svg.namespaceURI, 'circle');
    circle.setAttribute('cx', x(i)); circle.setAttribute('cy', y(i)); circle.setAttribute('r', '3'); circle.setAttribute('class', 'chart-point'); svg.append(circle);
  }
  figure.append(svg, el('figcaption', `${points[0].date} to ${points.at(-1).date} · ${points.length} recorded days · ${numeric(low)}–${numeric(high)} ${points[0].selected.unit}`, 'footnote'));
  return figure;
}

const seriesNames = {heart_rate: 'Heart rate', blood_glucose: 'Blood glucose', vo2_max: 'VO₂ Max', body_mass: 'Body weight', body_fat: 'Body fat', oxygen_saturation: 'Oxygen saturation', respiratory_rate: 'Respiratory rate'};
let selectedSeries = 'blood_glucose';
let heartRateParameters = {gap: 15, bpm: '', source: ''};
async function seriesView() {
  const wrap = el('div');
  const control = section('Recorded measurements');
  const choice = select(Object.entries(seriesNames), selectedSeries);
  choice.addEventListener('change', () => { selectedSeries = choice.value; navigate(); });
  control.append(field('Measurement', choice), el('p', `${selectedSeries === 'heart_rate' ? 'Last 24 hours' : 'Last 30 days'}. Sources and raw units remain available.`, 'muted'));
  if (selectedSeries === 'heart_rate') {
    const form = el('form');
    const gap = el('input'); gap.type = 'number'; gap.min = '1'; gap.max = '1800'; gap.required = true; gap.value = heartRateParameters.gap;
    const maximum = el('input'); maximum.type = 'number'; maximum.min = '50'; maximum.max = '300'; maximum.value = heartRateParameters.bpm;
    const source = el('input'); source.value = heartRateParameters.source; source.maxLength = 256;
    const apply = el('button', 'Apply sampling protocol'); apply.type = 'submit';
    form.append(field('Maximum sample gap in seconds (match your source protocol)', gap), field('Declared maximum heart rate (optional, bpm)', maximum), field('Maximum heart rate source', source), apply);
    form.onsubmit = event => { event.preventDefault(); heartRateParameters = {gap: Number(gap.value), bpm: maximum.value, source: source.value}; navigate(); };
    control.append(el('p', 'Heart rate uses the last 24 hours. The initial 15-second gap is an engineering assumption; set it to your source protocol.', 'footnote'), form);
  }
  wrap.append(control);
  const end = Math.floor(Date.now() / 1000);
  const result = await post('wellness/series', {record_type: selectedSeries, start_at: end - (selectedSeries === 'heart_rate' ? 1 : 30) * 86400, end_at: end, maximum_gap_seconds: selectedSeries === 'heart_rate' ? heartRateParameters.gap : 900, declared_maximum: selectedSeries === 'heart_rate' && heartRateParameters.bpm !== '' ? {bpm: Number(heartRateParameters.bpm), source: heartRateParameters.source} : null});
  if (!result.sources.length) wrap.append(empty('No recorded measurements in this window.'));
  for (const source of result.sources) {
    const panel = section(source.source); panel.append(badge(source.state));
    const valid = source.points.filter(p => p.value !== null);
    if (valid.length) {
      const latest = valid[valid.length - 1];
      panel.append(el('p', `${numeric(latest.value)} ${result.unit}`, 'metric-value'), el('p', `Last observation ${dated(latest.at)}`, 'muted'));
      panel.append(measurementPlot(valid, result.unit));
    }
    if (source.glucose_summary) {
      const summary = source.glucose_summary;
      panel.append(el('p', `Observed coverage: ${numeric(summary.coverage_fraction * 100)}% · ${numeric(summary.covered_seconds / 3600)} hours`, 'muted'));
      panel.append(table(['Measurement', 'Value'], [['Time in 70–180 mg/dL', summary.tir_percent === null ? 'Not available' : `${numeric(summary.tir_percent)}%`], ['Time-weighted mean', summary.mean_mg_dl === null ? 'Not available' : `${numeric(summary.mean_mg_dl)} mg/dL`], ['Coefficient of variation', summary.coefficient_of_variation === null ? 'Not available' : `${numeric(summary.coefficient_of_variation)}%`]]));
    }
    if (source.heart_rate_summary) {
      const heart = source.heart_rate_summary;
      panel.append(badge(heart.state), el('p', `Observed coverage: ${numeric(heart.coverage_fraction * 100)}%`, 'muted'));
      panel.append(table(['Absolute interval', 'Observed minutes'], heart.absolute_bins.map(b => [`${b.lower_bpm}–${b.upper_bpm} bpm`, numeric(b.seconds / 60)])));
      if (heart.zone_seconds) panel.append(table(['Relative zone', 'Observed minutes'], heart.zone_seconds.map((seconds, index) => [`${50 + index * 10}–${60 + index * 10}%`, numeric(seconds / 60)])));
      panel.append(el('p', heart.edwards_load_au == null ? 'Edwards load unavailable' : `Observed Edwards load: ${numeric(heart.edwards_load_au)} AU`), detail('Heart-rate method', heart));
    }
    const records = el('details'); records.append(el('summary', `${source.points.length} observations; latest 500 with original units`));
    records.append(table(['Observed', 'Converted', 'Original', 'Revision'], source.points.slice(-500).map(p => [dated(p.at), p.value === null ? 'Unavailable' : `${numeric(p.value)} ${result.unit}`, `${p.raw_value ?? 'Unknown'} ${p.raw_unit ?? ''}`, p.version])));
    panel.append(records); wrap.append(panel);
  }
  const evidence = section('Method and coverage');
  for (const note of selectedSeries === 'heart_rate' ? ['Heart-rate sources stay separate. Each source includes its interval method and declared maximum.'] : result.notes) evidence.append(el('p', note, 'footnote'));
  if (selectedSeries === 'blood_glucose') evidence.append(el('p', 'Glucose summaries hold each sample until the next sample only when the gap is at most 15 minutes. Long gaps remain missing.', 'footnote'));
  evidence.append(button('Download calculation and inputs', () => {
    const url = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], {type: 'application/json'}));
    const link = el('a'); link.href = url; link.download = `helpyourself-${result.record_type}-${end}.json`; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, 'quiet'));
  wrap.append(evidence); return wrap;
}
function measurementPlot(points, unit) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 600 180'); svg.setAttribute('role', 'img'); svg.setAttribute('aria-label', `Recorded values in ${unit}. Points do not imply continuous coverage.`);
  const first = points[0].at, last = points[points.length - 1].at;
  const low = Math.min(...points.map(p => p.value)), high = Math.max(...points.map(p => p.value));
  // Cap visible marks while retaining every observation in the table and download.
  const stride = Math.max(1, Math.ceil(points.length / 1500));
  for (let i = 0; i < points.length; i += stride) {
    const p = points[i], dot = document.createElementNS(svg.namespaceURI, 'circle');
    dot.setAttribute('cx', String(20 + 560 * (p.at - first) / (last - first || 1))); dot.setAttribute('cy', String(155 - 130 * (p.value - low) / (high - low || 1)));
    dot.setAttribute('r', '2.5'); dot.setAttribute('fill', 'currentColor'); svg.append(dot);
  }
  const wrap = el('figure'); wrap.append(svg, el('figcaption', `${numeric(low)}–${numeric(high)} ${unit} · ${dated(first)} to ${dated(last)} · ${stride > 1 ? 'Plot samples points; download includes every record.' : 'Every observation shown.'}`, 'footnote')); return wrap;
}

async function timelineView() {
  const wrap = el('div'), panel = section('Last 7 days');
  wrap.append(panel);
  const end = Math.floor(Date.now() / 1000), start = end - 7 * 86400;
  let cursor = null;
  const more = actionButton('Load more events', load, 'quiet');
  async function load() {
    const data = await post('wellness/timeline', {start_at: start, end_at: end, cursor});
    if (!data.events.length && cursor === null) panel.append(empty('No archived events in this window.'));
    for (const item of data.events) {
      const row = el('div', null, 'record-row');
      row.append(el('p', `${dated(item.at)} · ${item.record_type.replaceAll('_', ' ')}`), el('p', `${item.source} · revision ${item.version} · ${item.time_semantics === 'upload' ? 'Uploaded' : 'Record starts'}`, 'footnote'));
      row.append(button('Open details', () => item.record_type === 'report' ? route('reports', item.record_id) : route(item.source === 'manual:helpyourself' ? 'entries' : item.record_type === 'sleep' ? 'sleep' : 'health'), 'quiet')); panel.append(row);
    }
    cursor = data.next_cursor;
    more.hidden = !cursor;
  }
  await load(); wrap.append(more, el('p', 'Report dates are upload dates. Sleep stages remain individual original intervals. Device measurements have a separate trend view.', 'footnote')); return wrap;
}

async function reviewView() {
  const wrap = el('div');
  const end = Math.floor(Date.now() / 1000);
  const data = await post('wellness/review', {start_at: end - 30 * 86400, end_at: end, timezone});
  wrap.append(button('Record or edit a log', () => route('entries'), 'quiet'), el('p', `Last 30 days · ${timezone}`, 'muted'));
  if (!data.days.length) wrap.append(empty('No manual logs in this window.'));
  const training = section('Training load · completed calendar days');
  training.append(table(['Window', 'Recorded load', 'Complete days', 'Daily mean'], data.training.windows.map(w => [`${w.days} days`, w.observed_sum_au == null ? 'Unknown' : `${numeric(w.observed_sum_au)} AU`, `${w.complete_days}/${w.days}`, w.daily_mean_au == null ? 'Unknown' : `${numeric(w.daily_mean_au)} AU`])));
  training.append(table(['Date', 'State', 'Load'], data.training.days.map(d => [d.date, d.state.replaceAll('_', ' '), d.load_au == null ? 'Unknown' : `${numeric(d.load_au)} AU`])));
  training.append(detail('Training method and input references', data.training)); wrap.append(training);
  for (const kind of ['training', 'nutrition', 'breathing']) {
    const days = data.days.filter(d => d.kind === kind);
    if (!days.length) continue;
    const panel = section(kind[0].toUpperCase() + kind.slice(1));
    panel.append(table(['Date', 'Metric', 'Observed total', 'Missing fields'], days.flatMap(d => d.totals.map(t => [d.date, t.metric.replaceAll('_', ' '), t.observed_sum === null ? 'Not available' : `${numeric(t.observed_sum)} ${t.unit}`, `${t.missing_count} of ${d.record_count} logs`]))));
    wrap.append(panel);
  }
  if (data.strength_bests.length) {
    const panel = section('Recorded strength bests in this window');
    panel.append(table(['Exercise', 'Repetitions', 'External weight', 'Recorded'], data.strength_bests.map(b => [b.exercise, b.repetitions, `${numeric(b.external_weight_kg)} kg`, dated(b.at)]))); wrap.append(panel);
  }
  for (const [kind, metrics] of [['body', [['weight_kg', 'Weight', 'kg'], ['body_fat_percent', 'Body fat', '%'], ['waist_cm', 'Waist', 'cm']]], ['blood_pressure', [['systolic_mmhg', 'Systolic', 'mmHg'], ['diastolic_mmhg', 'Diastolic', 'mmHg']]]]) {
    const records = data.measurements.filter(r => r.entry.kind === kind);
    if (!records.length) continue;
    const panel = section(kind === 'body' ? 'Body measurements' : 'Paired blood pressure');
    for (const [key, label, unit] of metrics) {
      const points = records.filter(r => r.entry.content[key] !== null).map(r => ({at: r.at, value: r.entry.content[key]})).sort((a, b) => a.at - b.at);
      if (points.length) { panel.append(el('h3', label), measurementPlot(points, unit)); }
    }
    panel.append(table(['Recorded', ...metrics.map(m => m[1])], records.map(r => [dated(r.at), ...metrics.map(([key, , unit]) => r.entry.content[key] === null ? 'Unknown' : `${numeric(r.entry.content[key])} ${unit}`)]))); wrap.append(panel);
  }
  const cycles = data.measurements.filter(r => r.entry.kind === 'cycle');
  if (cycles.length) { const panel = section('Cycle observations'); panel.append(table(['Recorded', 'Flow', 'Context', 'Symptoms'], cycles.map(r => [dated(r.at), r.entry.content.flow, r.entry.content.context, r.entry.content.symptoms.join(', ')]))); wrap.append(panel); }
  for (const note of data.notes) wrap.append(el('p', note, 'footnote'));
  wrap.append(button('Download review and inputs', () => {
    const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], {type: 'application/json'})); const link = el('a'); link.href = url; link.download = `helpyourself-log-review-${end}.json`; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, 'quiet'));
  return wrap;
}

function breathingView() {
  const wrap = el('div'), panel = section('Cyclic sighing');
  panel.append(el('p', 'Inhale gently through your nose, then take a second small inhale. Exhale slowly through your mouth. Repeat at a comfortable pace. Stop if you feel uncomfortable.'));
  const source = el('a', 'Study and method'); source.href = 'https://pubmed.ncbi.nlm.nih.gov/36630953/'; source.target = '_blank'; source.rel = 'noopener noreferrer'; panel.append(source);
  panel.append(el('p', 'The timer records practice duration. It does not measure breathing rate, stress or recovery.', 'footnote'));
  const clock = el('p', '5:00', 'metric-value'); clock.setAttribute('role', 'timer'); panel.append(clock);
  let began = null, at = null, elapsed = 0, timer = null, recordID = null, batchID = null;
  const start = button('Start five minutes', () => {
    began = performance.now(); at = Math.floor(Date.now() / 1000); elapsed = 0; recordID = crypto.randomUUID(); batchID = crypto.randomUUID();
    start.disabled = true; stop.disabled = false; save.hidden = true; status.textContent = 'Practice at your own pace.';
    timer = setInterval(() => { update(); if (elapsed >= 300) finish(); }, 250);
  }, 'primary');
  function update() {
    elapsed = Math.min(300, Math.max(0, (performance.now() - began) / 1000));
    const remaining = Math.ceil(300 - elapsed); clock.textContent = `${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}`;
  }
  function finish() {
    if (timer === null) return;
    update(); clearInterval(timer); timer = null; start.disabled = false; stop.disabled = true;
    save.hidden = elapsed < 0.6; status.textContent = `${numeric(elapsed / 60)} minutes recorded. Save only if you practiced during this time.`;
  }
  const stop = button('Stop practice', finish, 'quiet'); stop.disabled = true;
  const status = el('p', 'The timer stops when this page becomes hidden.', 'muted'); status.setAttribute('role', 'status');
  const save = actionButton('Save practice log', async () => {
    await post('wellness/entries/save', {record_id: recordID, version: 1, batch_id: batchID, at, timezone, entry: {kind: 'breathing', content: {duration_minutes: elapsed / 60, breaths_per_minute: null, note: 'Cyclic sighing guide. Foreground practice timer. Breathing rate was not measured.'}}});
    save.hidden = true; status.textContent = 'Practice saved to daily logs.';
  }, 'primary'); save.hidden = true;
  const signal = controller.signal;
  document.addEventListener('visibilitychange', () => { if (document.hidden) finish(); }, {signal});
  signal.addEventListener('abort', () => { if (timer !== null) clearInterval(timer); }, {once: true});
  const controls = el('div', null, 'toolbar'); controls.append(start, stop, save); panel.append(controls, status); wrap.append(panel); return wrap;
}


async function associationsView() {
  const wrap = el('div'), panel = section('Declare an analysis');
  panel.append(el('p', 'Exploratory statistics, not causal effects. Choose the entire comparison family before running. Repeated searches are not covered by the correction.', 'notice'));
  const [sources, saved] = await Promise.all([post('wellness/sources', {}), post('wellness/associations/list', {})]);
  const form = el('form'), outcome = select([['sleep', 'Recorded sleep seconds'], ['hrv_sdnn', 'HRV SDNN (not RMSSD)'], ['resting_heart_rate', 'Resting heart rate'], ['heart_rate', 'Heart rate'], ['workout', 'Recorded workout seconds'], ['steps', 'Recorded steps']], 'sleep');
  const source = el('select');
  function refreshSources() {
    source.replaceChildren();
    for (const item of sources.sources.filter(item => item.record_type === outcome.value)) { const option = el('option', `${item.platform}:${item.source_id}`); option.value = `${item.platform}:${item.source_id}`; source.append(option); }
  }
  outcome.onchange = refreshSources; refreshSources();
  const end = el('input'); end.type = 'date'; end.required = true;
  const yesterday = new Date(); yesterday.setDate(yesterday.getDate() - 1); end.value = dateScope(1, yesterday, timezone).end_date;
  const behaviors = el('textarea'); behaviors.required = true; behaviors.placeholder = 'caffeine\nalcohol';
  const covariates = el('textarea'); covariates.placeholder = 'illness';
  const lag = select([['0', 'Same date'], ['1', 'Behavior on previous date']], '1');
  const consent = el('input'); consent.type = 'checkbox'; consent.required = true;
  const run = el('button', 'Run declared analysis', 'primary'); run.type = 'submit';
  form.append(field('Outcome', outcome), field('One source for the whole window', source), field('Last completed date in the 90-day window', end), field('Behavior names (one per line, up to 8)', behaviors), field('Covariate names (one per line, up to 3)', covariates), field('Declared lag', lag), field('I selected this family before inspecting results and will check source protocol changes and missing data.', consent), run);
  const output = section('Result'); output.append(empty('Run a declared analysis or open a saved result.'));
  const names = input => input.value.split('\n').map(s => s.trim()).filter(Boolean);
  function show(result) {
    output.replaceChildren(el('h2', 'Result'), el('p', `Source: ${result.parameters.source} · End date: ${result.parameters.end_date}`, 'muted'));
    for (const row of result.results) {
      const card = el('article', null, 'observation'); card.append(el('h3', row.behavior), badge(row.state), el('p', `${row.sample_days}/90 complete days · ${row.missing_days} missing or excluded`));
      if (row.estimate) {
        const e = row.estimate;
        card.append(el('p', `Association: ${numeric(e.effect)} ${row.outcome_unit} per ${row.predictor_unit}`), el('p', `Nominal 95% interval: ${e.ci_95.map(numeric).join(' to ')} · nominal p: ${e.p_value.toPrecision(4)} · nominal BY q: ${row.q_value.toPrecision(4)}`), el('p', e.bootstrap_ci_95 ? `Block bootstrap 95% interval: ${e.bootstrap_ci_95.map(numeric).join(' to ')}` : 'Block bootstrap unavailable: too many singular resamples.'));
        card.append(el('p', 'Significance decisions are disabled: this fixed protocol failed synthetic error-rate calibration. p/q values and intervals are exploratory diagnostics.', 'notice'));
      }
      card.append(detail('Daily inclusion and missingness', row.days)); output.append(card);
    }
    for (const note of result.notes) output.append(el('p', note, 'footnote'));
    output.append(detail('Fixed model protocol', result.protocol), button('Download result and inputs', () => {
      const url = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], {type: 'application/json'})); const link = el('a'); link.href = url; link.download = `helpyourself-associations-${result.result_id}.json`; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
    }, 'quiet'));
  }
  form.onsubmit = event => { event.preventDefault(); action(run, async () => {
    const result = await post('wellness/associations/run', {end_date: end.value, timezone, outcome: outcome.value, source: source.value, behaviors: names(behaviors), covariates: names(covariates), lag_days: Number(lag.value)}); show(result);
  }); };
  panel.append(form); wrap.append(panel, output);
  const history = section('Saved current analyses');
  history.append(el('p', 'Source edits remove saved analyses and their input snapshots. Results are included in the full archive export.', 'footnote'));
  for (const item of saved.results) history.append(actionButton(`${item.parameters.outcome} · ${item.parameters.end_date} · ${dated(item.created_at)}`, async () => show(await post('wellness/associations/get', {result_id: item.result_id})), 'quiet'));
  wrap.append(history); return wrap;
}

async function hrvView() {
  const now = Math.floor(Date.now()/1000);
  const result = await post('wellness/hrv', {start_at: now-90*86400, end_at: now, timezone});
  const wrap = el('div');
  for (const note of result.notes) wrap.append(el('p', note, 'notice'));
  if (!result.sources.length) wrap.append(empty('No archived NN or heartbeat windows in the last 90 days.'));
  for (const source of result.sources) {
    const panel = section(source.source); panel.append(detail('Acquisition protocol', source.protocol));
    panel.append(table(['Date', 'lnRMSSD', 'Baseline days', 'Prior median', 'Prior MAD'], source.days.map(day => [day.date, numeric(day.ln_rmssd), `${day.baseline_days}/28`, numeric(day.baseline?.median), numeric(day.baseline?.mad)])));
    for (const window of source.windows) panel.append(detail(`${window.date} · ${window.state} · ${window.record_id}`, window));
    wrap.append(panel);
  }
  return wrap;
}
