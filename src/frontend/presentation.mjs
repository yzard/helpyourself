export function el(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== null && text !== undefined) node.textContent = String(text);
  if (className) node.className = className;
  return node;
}
export function button(text, action, className) {
  const node = el('button', text, className);
  node.type = 'button';
  node.addEventListener('click', action);
  return node;
}
export function badge(status) { return el('span', (status ?? 'unknown').replaceAll('_', ' '), 'badge'); }
export function dated(seconds) {
  return seconds ? new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : 'Not available';
}
export function numeric(value) {
  return value === null || value === undefined ? 'No visible samples' : Number(value).toLocaleString(undefined, { maximumFractionDigits: 3 });
}
export function quantity(value) {
  if (!value?.display) return value?.reason ?? 'Not available';
  return value.display;
}
export function originalReference(payload, reference) {
  if (!payload.reference_range) return 'Not printed';
  const origin = reference.unit_origin === 'reference_explicit' ? 'explicit unit' : 'result-column unit';
  return `${payload.reference_range} · ${reference.original_unit ?? 'unit unknown'}${reference.original_unit ? ` (${origin})` : ''}`;
}
export function detail(title, value) {
  const node = el('details');
  node.append(el('summary', title), el('pre', typeof value === 'string' ? value : JSON.stringify(value, null, 2)));
  return node;
}
export function table(headings, rows) {
  const wrap = el('div', null, 'table-wrap');
  const node = el('table');
  const head = el('thead');
  const tr = el('tr');
  for (const name of headings) {
    const cell = el('th', name);
    cell.scope = 'col';
    tr.append(cell);
  }
  head.append(tr);
  const body = el('tbody');
  for (const row of rows) {
    const tr = el('tr');
    for (const value of row) {
      const td = el('td');
      td.append(value instanceof Node ? value : el('span', value ?? '—'));
      tr.append(td);
    }
    body.append(tr);
  }
  node.append(head, body);
  wrap.append(node);
  return wrap;
}
// Every point remains a keyboard-accessible source link. Separate series preserve units.
export function chart(points, title, selected) {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  const width = window.matchMedia('(max-width: 700px)').matches ? 360 : 800;
  const right = width - 28;
  svg.setAttribute('viewBox', `0 0 ${width} 260`);
  svg.setAttribute('class', 'chart');
  svg.setAttribute('role', 'group');
  svg.setAttribute('aria-label', title);
  const add = (tag, attributes, text) => {
    const node = document.createElementNS(ns, tag);
    for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, value);
    if (text !== undefined) node.textContent = text;
    svg.append(node);
    return node;
  };
  const bounds = points.reduce((bounds, point) => ({
    low: Math.min(bounds.low, Number(point.value)), high: Math.max(bounds.high, Number(point.value)),
    first: Math.min(bounds.first, point.timestamp), last: Math.max(bounds.last, point.timestamp),
  }), { low: Infinity, high: -Infinity, first: Infinity, last: -Infinity });
  const { low, high, first, last } = bounds;
  const spread = high - low || Math.max(Math.abs(low) * 0.1, 1);
  const floor = low - spread * (high === low ? 0.5 : 0.15);
  const ceiling = high + spread * (high === low ? 0.5 : 0.15);
  const x = point => last === first ? (72 + right) / 2 : 72 + (point.timestamp - first) / (last - first) * (right - 72);
  const y = point => 210 - (Number(point.value) - floor) / (ceiling - floor) * 178;
  for (const fraction of [0, 0.5, 1]) {
    const value = floor + (ceiling - floor) * fraction;
    const py = y({ value });
    add('line', { x1: 72, x2: right, y1: py, y2: py, class: 'grid-line' });
    add('text', { x: 60, y: py + 4, 'text-anchor': 'end', class: 'chart-label' }, numeric(value));
  }
  const path = points.map((point, i) => `${i ? 'L' : 'M'} ${x(point)} ${y(point)}`).join(' ');
  add('path', { d: path, class: 'chart-line' });
  for (const point of points) {
    add('circle', { cx: x(point), cy: y(point), r: 7, class: 'chart-point', 'aria-hidden': 'true' });
    const circle = add('circle', {
      cx: x(point), cy: y(point), r: 18, class: 'chart-hit', tabindex: 0, role: 'button',
      'aria-label': `${point.sampled_at}: ${point.value} ${point.unit}. Open source report.`,
    });
    circle.addEventListener('click', () => selected(point));
    circle.addEventListener('keydown', event => {
      if (['Enter', ' '].includes(event.key)) { event.preventDefault(); selected(point); }
    });
  }
  for (const [timestamp, position, anchor] of [[first, 72, 'start'], [last, right, 'end']]) {
    add('text', { x: position, y: 248, 'text-anchor': anchor, class: 'chart-label' }, new Date(timestamp * 1000).toLocaleDateString(undefined, { timeZone: 'UTC', year: 'numeric', month: 'short', day: 'numeric' }));
  }
  return svg;
}
