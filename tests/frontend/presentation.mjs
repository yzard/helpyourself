import test from 'node:test';
import assert from 'node:assert/strict';
import { quantity, originalReference, numeric } from '../../src/frontend/presentation.mjs';

test('standardized values preserve the server display including exactly its provided unit', () => {
  assert.equal(quantity({ display: '120 mg/dL', unit: 'mg/dL' }), '120 mg/dL');
  assert.equal(quantity({ display: '< 100 mg/dL', unit: 'mg/dL' }), '< 100 mg/dL');
  assert.equal(quantity({ display: null, reason: 'Unsupported unit' }), 'Unsupported unit');
});

test('printed references retain explicit units independently from the result column', () => {
  assert.equal(originalReference({ reference_range: '<2.586 mmol/L', raw_unit: 'mg/dL' }, { original_unit: 'mmol/L', unit_origin: 'reference_explicit' }), '<2.586 mmol/L · mmol/L (explicit unit)');
  assert.equal(originalReference({ reference_range: '<100', raw_unit: 'mg/dL' }, { original_unit: 'mg/dL', unit_origin: 'observation' }), '<100 · mg/dL (result-column unit)');
  assert.equal(originalReference({ reference_range: null }, {}), 'Not printed');
});

test('missing health values remain missing and real zeroes remain visible', () => {
  assert.equal(numeric(null), 'No visible samples');
  assert.equal(numeric(0), '0');
});
