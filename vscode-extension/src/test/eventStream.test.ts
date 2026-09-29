import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import { findContractId, formatEvent, isContractId, SseParser } from '../eventStream';

const CONTRACT = 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC';

test('parses complete messages and ignores keep-alive comments', () => {
    const parser = new SseParser();
    const out = parser.push(': keep-alive\n\nid: 7\ndata: {"a":1}\n\n');
    assert.deepEqual(out, [{ id: '7', event: 'message', data: '{"a":1}' }]);
    assert.equal(parser.lastEventId, '7');
});

test('buffers messages split across chunks, including a split CRLF', () => {
    const parser = new SseParser();
    assert.deepEqual(parser.push('id: 1\r\nda'), []);
    assert.deepEqual(parser.push('ta: {"b":'), []);
    assert.deepEqual(parser.push('2}\r'), []);
    assert.deepEqual(parser.push('\n\r\n'), [{ id: '1', event: 'message', data: '{"b":2}' }]);
});

test('joins multi-line data and honours named events and retry', () => {
    const parser = new SseParser();
    const out = parser.push('event: lag\nretry: 3000\ndata: line1\ndata:line2\n\n');
    assert.deepEqual(out, [{ id: undefined, event: 'lag', data: 'line1\nline2' }]);
});

test('an event with no data is not dispatched', () => {
    const parser = new SseParser();
    assert.deepEqual(parser.push('id: 9\n\n'), []);
    assert.equal(parser.lastEventId, '9');
});

test('detects contract ids in a selection', () => {
    assert.equal(findContractId(`const id = "${CONTRACT}";`), CONTRACT);
    assert.equal(findContractId('GAAAA not a contract'), undefined);
    assert.equal(isContractId(` ${CONTRACT} `), true);
    assert.equal(isContractId(CONTRACT.slice(1)), false);
});

test('formats JSON events with a summary header', () => {
    const now = new Date('2026-01-01T00:00:00Z');
    const text = formatEvent({ event: 'message', data: '{"event_type":"contract","ledger":42,"tx_hash":"ab"}' }, now);
    assert.ok(text.startsWith('[2026-01-01T00:00:00.000Z]  contract  ledger 42  tx ab\n{\n  "event_type"'));
});

test('falls back to raw data when the payload is not JSON', () => {
    const now = new Date('2026-01-01T00:00:00Z');
    assert.equal(formatEvent({ event: 'message', data: 'hello' }, now), '[2026-01-01T00:00:00.000Z] message\nhello\n');
});
