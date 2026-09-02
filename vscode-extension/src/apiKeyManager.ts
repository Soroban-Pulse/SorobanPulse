import * as vscode from 'vscode';

// ---------------------------------------------------------------------------
// API key management — Issue #963
//
// Previously the only place to put an API key was the plain
// `sorobanpulse.apiKey` setting, which VS Code persists in settings.json —
// readable by anything with filesystem access, synced in cleartext if
// Settings Sync is on, and easy to accidentally commit in a workspace
// settings file. This module stores the key in VS Code's SecretStorage
// (backed by the OS keychain) instead, via a command, while still falling
// back to the legacy setting so existing users aren't broken.
// ---------------------------------------------------------------------------

const SECRET_KEY_API = 'sorobanpulse.apiKey';
const SECRET_KEY_ADMIN = 'sorobanpulse.adminApiKey';

export async function getApiKey(context: vscode.ExtensionContext): Promise<string> {
    const stored = await context.secrets.get(SECRET_KEY_API);
    if (stored) { return stored; }
    // Legacy fallback for keys set before secret storage was introduced.
    return vscode.workspace.getConfiguration('sorobanpulse').get<string>('apiKey', '');
}

export async function getAdminApiKey(context: vscode.ExtensionContext): Promise<string> {
    const stored = await context.secrets.get(SECRET_KEY_ADMIN);
    if (stored) { return stored; }
    return vscode.workspace.getConfiguration('sorobanpulse').get<string>('adminApiKey', '');
}

export async function setApiKey(context: vscode.ExtensionContext): Promise<void> {
    const value = await vscode.window.showInputBox({
        prompt: 'Soroban Pulse API key (sent as the x-api-key header)',
        password: true,
        ignoreFocusOut: true,
        placeHolder: 'sk_live_...',
    });
    if (value === undefined) { return; } // cancelled
    if (value.length === 0) {
        vscode.window.showWarningMessage('API key was empty — nothing saved.');
        return;
    }
    await context.secrets.store(SECRET_KEY_API, value);
    vscode.window.showInformationMessage('Soroban Pulse API key saved securely.');
}

export async function setAdminApiKey(context: vscode.ExtensionContext): Promise<void> {
    const value = await vscode.window.showInputBox({
        prompt: 'Soroban Pulse admin API key (for /admin/* endpoints)',
        password: true,
        ignoreFocusOut: true,
    });
    if (value === undefined) { return; }
    if (value.length === 0) {
        vscode.window.showWarningMessage('Admin API key was empty — nothing saved.');
        return;
    }
    await context.secrets.store(SECRET_KEY_ADMIN, value);
    vscode.window.showInformationMessage('Soroban Pulse admin API key saved securely.');
}

export async function clearApiKeys(context: vscode.ExtensionContext): Promise<void> {
    await context.secrets.delete(SECRET_KEY_API);
    await context.secrets.delete(SECRET_KEY_ADMIN);
    vscode.window.showInformationMessage('Soroban Pulse API keys cleared from secure storage.');
}
