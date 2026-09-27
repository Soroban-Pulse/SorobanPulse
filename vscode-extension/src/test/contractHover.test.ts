import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import { CONTRACT_ID_PATTERN, ContractSummary, renderSummary, SummaryCache } from '../contractHover';

const ID = 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC';
const SUMMARY: ContractSummary = {
    contract_id: ID,
    total_events: 12345,
    last_event_at: '2026-01-01T00:00:00Z',
    ledger_range: { min: 1, max: 777 },
};

test('pattern matches a full contract id and nothing shorter', () => {
    assert.equal(new RegExp(`^${CONTRACT_ID_PATTERN.source}$`).test(ID), true);
    assert.equal(new RegExp(`^${CONTRACT_ID_PATTERN.source}$`).test(ID.slice(0, 55)), false);
    assert.equal(CONTRACT_ID_PATTERN.test('GDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC'), false);
});

test('cache serves repeat hovers from one request until the ttl expires', async () => {
    let clock = 0;
    let calls = 0;
    const cache = new SummaryCache(async () => { calls++; return SUMMARY; }, 1000, () => clock);

    await Promise.all([cache.get(ID), cache.get(ID)]);
    clock = 999;
    assert.deepEqual(await cache.get(ID), SUMMARY);
    assert.equal(calls, 1);

    clock = 1000;
    await cache.get(ID);
    assert.equal(calls, 2);
});

test('fetch failures resolve to undefined and are cached', async () => {
    let calls = 0;
    const cache = new SummaryCache(async () => { calls++; throw new Error('ECONNREFUSED'); }, 1000, () => 0);

    assert.equal(await cache.get(ID), undefined);
    assert.equal(await cache.get(ID), undefined);
    assert.equal(calls, 1);
});

test('renders totals, last ledger and last event time', () => {
    const md = renderSummary(SUMMARY);
    assert.ok(md.includes(`\`${ID}\``));
    assert.ok(md.includes('Total events: 12,345'));
    assert.ok(md.includes('Last seen ledger: 777'));
    assert.ok(md.includes('Last event: 2026-01-01T00:00:00Z'));
});

test('omits ledger and time lines for a contract with no events', () => {
    const md = renderSummary({ contract_id: ID, total_events: 0, last_event_at: null, ledger_range: { min: null, max: null } });
    assert.ok(md.includes('Total events: 0'));
    assert.ok(!md.includes('Last seen ledger'));
    assert.ok(!md.includes('Last event'));
});
