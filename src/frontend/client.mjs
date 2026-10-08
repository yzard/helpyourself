const READ_ACTIONS = new Set([
  'wellness/hrv', 'user/get', 'server/status', 'reports/list', 'reports/get', 'reports/input/get',
  'reports/extraction/get', 'observations/history', 'metrics/list', 'trends/get',
  'wellness/associations/list', 'wellness/associations/get', 'wellness/review', 'wellness/timeline', 'wellness/sleep/regularity', 'wellness/series', 'wellness/preferences/get', 'wellness/import/list', 'wellness/clinical-age', 'wellness/sleep', 'wellness/entries/list', 'wellness/day', 'wellness/sources', 'exports/list', 'health/coverage', 'health/list', 'health/aggregate', 'analysis/list', 'analysis/get',
]);
const ACTIONS = new Set([...READ_ACTIONS, 'wellness/associations/run', 'session/login', 'session/logout', 'analysis/create', 'analysis/retry', 'exports/create', 'exports/delete', 'wellness/preferences/save', 'wellness/import/delete', 'wellness/import/fit', 'wellness/import/tcx', 'wellness/import/gpx', 'wellness/entries/save', 'wellness/entries/delete']);

export class APIError extends Error {
  constructor(message, status) {
    super(message);
    this.status = status;
  }
}

// The browser owns only a memory session. All requests stay on the page's API origin.
export class Client {
  constructor(fetcher, expired) {
    this.fetcher = fetcher;
    this.expired = expired;
    this.session = null;
  }
  clear() { this.session = null; }
  async post(action, body, signal) {
    if (!ACTIONS.has(action)) throw new Error('This action is unavailable in the web viewer.');
    const response = await this.request(`/api/v1/${action}`, {
      method: 'POST', body: JSON.stringify(body), headers: { 'Content-Type': 'application/json' }, signal,
    }, action === 'session/login' || action === 'server/status');
    return response.json();
  }
  async original(fileID, signal) {
    if (!fileID || !/^[a-zA-Z0-9-]+$/.test(fileID)) throw new Error('Invalid report identifier.');
    const response = await this.request(`/api/v1/files/${encodeURIComponent(fileID)}/download`, {
      method: 'GET', headers: {}, signal,
    }, false);
    const blob = await response.blob();
    if (!['image/png', 'image/jpeg', 'image/heic', 'image/heif', 'application/pdf'].includes(blob.type)) {
      throw new Error('This original format cannot be previewed.');
    }
    return blob;
  }
  async exportArchive(id, signal) {
    if (typeof id !== 'string' || !/^[a-zA-Z0-9-]+$/.test(id)) throw new Error('Invalid export identifier.');
    const response = await this.request(`/api/v1/exports/${id}/download`, {
      method: 'GET', headers: {}, signal,
    }, false);
    return response.blob();
  }
  async request(path, options, anonymous) {
    const session = this.session;
    if (!anonymous && (!session || session.expires_at * 1000 <= Date.now())) {
      this.clear();
      this.expired();
      throw new APIError('Your session has expired. Sign in again.', 401);
    }
    const headers = { ...options.headers };
    if (!anonymous) headers.Authorization = `Bearer ${session.token}`;
    const response = await this.fetcher(path, {
      ...options, headers, cache: 'no-store', credentials: 'omit', redirect: 'error',
      signal: AbortSignal.any([options.signal ?? new AbortController().signal, AbortSignal.timeout(60000)]),
    });
    if (!response.ok) {
      if (response.status === 401 && !anonymous && this.session === session) {
        this.clear();
        this.expired();
      }
      const data = await response.json().catch(() => null);
      throw new APIError(data?.error?.message ?? `Request failed (${response.status}).`, response.status);
    }
    return response;
  }
}

export function dateScope(days, now, timezone) {
  const start = new Date(now);
  start.setDate(start.getDate() - days + 1);
  const date = value => `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, '0')}-${String(value.getDate()).padStart(2, '0')}`;
  return { start_date: date(start), end_date: date(now), timezone };
}

export function safeLink(value) {
  try {
    const url = new URL(value);
    return ['https:', 'http:'].includes(url.protocol) ? url.href : null;
  } catch { return null; }
}

export function sessionDelay(expiresAt, now) {
  return Math.max(0, Math.min(expiresAt * 1000 - now, 2147483647));
}
