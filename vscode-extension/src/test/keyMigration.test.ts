import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import { describeMigration, LegacySettings, migrateLegacyKeys, SecretStore } from '../keyMigration';

function fakeSecrets(initial: Record<string, string> = {}): SecretStore & { data: Map<string, string> } {
    const data = new Map(Object.entries(initial));
    return {
        data,
        get: async key => data.get(key),
        store: async (key, value) => { data.set(key, value); },
    };
}

function fakeSettings(initial: Record<string, string>): LegacySettings & { data: Map<string, string> } {
    const data = new Map(Object.entries(initial));
    return {
        data,
        read: setting => data.get(setting) ?? '',
        clear: async setting => { data.delete(setting); },
    };
}

test('moves plaintext keys into secret storage and clears settings', async () => {
    const secrets = fakeSecrets();
    const settings = fakeSettings({ apiKey: 'sk_user', adminApiKey: 'sk_admin' });

    const result = await migrateLegacyKeys(secrets, settings);

    assert.deepEqual(result, { migrated: ['apiKey', 'adminApiKey'], discarded: [] });
    assert.equal(secrets.data.get('sorobanpulse.apiKey'), 'sk_user');
    assert.equal(secrets.data.get('sorobanpulse.adminApiKey'), 'sk_admin');
    assert.equal(settings.data.size, 0);
});

test('keeps an existing secret and still removes the stale setting', async () => {
    const secrets = fakeSecrets({ 'sorobanpulse.apiKey': 'sk_new' });
    const settings = fakeSettings({ apiKey: 'sk_old' });

    const result = await migrateLegacyKeys(secrets, settings);

    assert.deepEqual(result, { migrated: [], discarded: ['apiKey'] });
    assert.equal(secrets.data.get('sorobanpulse.apiKey'), 'sk_new');
    assert.equal(settings.data.has('apiKey'), false);
});

test('is a no-op when no plaintext keys are configured', async () => {
    const secrets = fakeSecrets();
    let cleared = 0;
    const settings: LegacySettings = { read: () => '', clear: async () => { cleared++; } };

    const result = await migrateLegacyKeys(secrets, settings);

    assert.deepEqual(result, { migrated: [], discarded: [] });
    assert.equal(cleared, 0);
    assert.equal(describeMigration(result), undefined);
});

test('notification names every setting that left settings.json', () => {
    const message = describeMigration({ migrated: ['apiKey'], discarded: ['adminApiKey'] });
    assert.ok(message?.includes('sorobanpulse.apiKey'));
    assert.ok(message?.includes('sorobanpulse.adminApiKey'));
});
