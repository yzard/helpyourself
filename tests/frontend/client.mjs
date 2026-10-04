import test from 'node:test';
import assert from 'node:assert/strict';
import { Client, APIError, dateScope, safeLink, sessionDelay } from '../../src/frontend/client.mjs';

function session(token) { return { token, expires_at: Math.floor(Date.now() / 1000) + 300 }; }

test('viewer refuses import, review, deletion, arbitrary paths, and invalid original IDs before any fetch', async () => {
  let calls = 0;
  const client = new Client(async () => { calls++; }, () => {});
  client.session = session('test');
  for (const action of ['files/upload', 'health/connect', 'health/sync', 'reports/review', 'reports/delete', 'user/delete', 'exports/create', '../session/login', 'https://other-server.test']) {
    await assert.rejects(client.post(action, {}, undefined), /unavailable/);
  }
  for (const id of ['../secret', 'abc?token=secret', '']) await assert.rejects(client.original(id, undefined), /Invalid/);
  assert.equal(calls, 0);
});

test('reads and presets use the existing same-origin Bearer API without persistence or cookies', async () => {
  const calls = [];
  const client = new Client(async (url, options) => {
    calls.push([url, options]);
    return Response.json({ runs: [] });
  }, () => {});
  client.session = session('test-token');
  await client.post('analysis/list', {}, undefined);
  await client.post('analysis/create', { start_date: '2026-01-01', end_date: '2026-03-31', timezone: 'UTC' }, undefined);
  assert.equal(calls[0][0], '/api/v1/analysis/list');
  assert.equal(calls[1][0], '/api/v1/analysis/create');
  assert.equal(calls[0][1].headers.Authorization, 'Bearer test-token');
  assert.equal(calls[0][1].credentials, 'omit');
  assert.equal(calls[0][1].cache, 'no-store');
  assert.equal(calls[0][1].redirect, 'error');
  assert.equal(calls[0][1].body, '{}');
  await client.post('session/login', { username: 'alice', password: 'synthetic' }, undefined);
  assert.equal(calls[2][1].headers.Authorization, undefined);
});

test('expired sessions and server revocation clear local authorization', async () => {
  let expired = 0, requests = 0;
  const client = new Client(async () => {
    requests++;
    return Response.json({ error: { message: 'Session revoked' } }, { status: 401 });
  }, () => { expired++; });
  client.session = { token: 'old', expires_at: 1 };
  await assert.rejects(client.post('reports/list', { limit: 100 }, undefined), APIError);
  assert.equal(requests, 0);
  assert.equal(client.session, null);
  client.session = session('revoked');
  await assert.rejects(client.post('analysis/list', {}, undefined), /Session revoked/);
  assert.equal(client.session, null);
  assert.equal(expired, 2);
});

test('a late 401 from an old session cannot revoke a newer browser login', async () => {
  let finish;
  const client = new Client(() => new Promise(resolve => { finish = resolve; }), () => assert.fail('New session was cleared'));
  client.session = session('old');
  const request = client.post('analysis/list', {}, undefined);
  client.session = session('new');
  finish(Response.json({ error: { message: 'Expired' } }, { status: 401 }));
  await assert.rejects(request, APIError);
  assert.equal(client.session.token, 'new');
});

test('provider and download errors remain actionable; originals are restricted to supported report types', async () => {
  let result = Response.json({ error: { message: 'Analysis provider is disabled' } }, { status: 400 });
  const client = new Client(async () => result, () => {});
  client.session = session('test');
  await assert.rejects(client.post('analysis/create', {}, undefined), /provider is disabled/);
  result = new Response('<script>secret</script>', { headers: { 'Content-Type': 'text/html' } });
  await assert.rejects(client.original('report-123', undefined), /cannot be previewed/);
  result = new Response('synthetic-pdf', { headers: { 'Content-Type': 'application/pdf' } });
  assert.equal((await client.original('report-123', undefined)).type, 'application/pdf');
});

test('date scopes include exactly the selected calendar days and preserve the browser timezone', () => {
  const now = new Date(2026, 0, 5, 12);
  assert.deepEqual(dateScope(30, now, 'America/New_York'), { start_date: '2025-12-07', end_date: '2026-01-05', timezone: 'America/New_York' });
  assert.deepEqual(dateScope(90, new Date(2026, 9, 3, 12), 'UTC'), { start_date: '2026-07-06', end_date: '2026-10-03', timezone: 'UTC' });
});

test('AI evidence links reject script, local-file, malformed, and relative URLs', () => {
  for (const value of ['javascript:alert(1)', 'data:text/html,unsafe', 'file:///data/secret', '/api/v1/user/get', 'bad url']) assert.equal(safeLink(value), null);
  assert.equal(safeLink('https://medlineplus.gov/lab-tests/cholesterol-levels/'), 'https://medlineplus.gov/lab-tests/cholesterol-levels/');
});

test('long server sessions stay within browser timer limits instead of immediately expiring', () => {
  const now = 1800000000000;
  assert.equal(sessionDelay(now / 1000 + 31536000, now), 2147483647);
  assert.equal(sessionDelay(now / 1000 + 60, now), 60000);
  assert.equal(sessionDelay(now / 1000 - 1, now), 0);
});
