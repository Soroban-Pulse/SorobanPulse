// ---------------------------------------------------------------------------
// Legacy API key migration (Issue #1123)
//
// Keys set before SecretStorage existed live in plaintext settings.json
// (user, workspace or folder scope). On activation we move each one into
// SecretStorage and clear it from every scope it was written to, so that
// after the first run no key material is left in settings.
//
// Kept free of any `vscode` import so it can be unit tested under plain
// Node; `extension.ts` adapts the real VS Code APIs to these interfaces.
// ---------------------------------------------------------------------------

export interface SecretStore {
    get(key: string): PromiseLike<string | undefined>;
    store(key: string, value: string): PromiseLike<void>;
}

export interface LegacySettings {
    /** Plaintext value from the most specific scope that sets it, or '' when unset. */
    read(setting: string): string;
    /** Remove the setting from every scope that defines it. */
    clear(setting: string): PromiseLike<void>;
}

export interface LegacyKey {
    /** Setting name under the `sorobanpulse` section, e.g. `apiKey`. */
    setting: string;
    /** SecretStorage key the value is moved to. */
    secret: string;
}

export const LEGACY_KEYS: readonly LegacyKey[] = [
    { setting: 'apiKey', secret: 'sorobanpulse.apiKey' },
    { setting: 'adminApiKey', secret: 'sorobanpulse.adminApiKey' },
];

export interface MigrationResult {
    /** Settings whose value was copied into SecretStorage. */
    migrated: string[];
    /** Settings cleared without copying because a secret already existed. */
    discarded: string[];
}

export async function migrateLegacyKeys(
    secrets: SecretStore,
    settings: LegacySettings,
    keys: readonly LegacyKey[] = LEGACY_KEYS,
): Promise<MigrationResult> {
    const result: MigrationResult = { migrated: [], discarded: [] };
    for (const { setting, secret } of keys) {
        const plaintext = settings.read(setting);
        if (plaintext.length === 0) { continue; }

        // A key already stored via the command wins over a stale setting.
        const existing = await secrets.get(secret);
        if (existing) {
            result.discarded.push(setting);
        } else {
            await secrets.store(secret, plaintext);
            result.migrated.push(setting);
        }
        await settings.clear(setting);
    }
    return result;
}

export function describeMigration(result: MigrationResult): string | undefined {
    const moved = [...result.migrated, ...result.discarded];
    if (moved.length === 0) { return undefined; }
    const names = moved.map(s => `sorobanpulse.${s}`).join(', ');
    return `Soroban Pulse moved ${names} out of settings.json into secure storage.`;
}
