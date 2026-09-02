import * as vscode from 'vscode';
import { makeRequest } from './requestTester';

// ---------------------------------------------------------------------------
// Webhook test interface — Issue #963
//
// Sends a synthetic event payload directly to a callback URL — the same
// shape a real subscription delivery uses — so a user can confirm their
// receiver is reachable and returns 2xx before pointing a live subscription
// at it. Mirrors `spulse webhook-test` in the CLI (cli/src/webhook_test.rs).
// ---------------------------------------------------------------------------

function samplePayload(contractId: string): string {
    return JSON.stringify(
        {
            id: '00000000-0000-4000-8000-000000000000',
            contract_id: contractId,
            event_type: 'contract',
            tx_hash: '0'.repeat(64),
            ledger: 1,
            timestamp: new Date().toISOString(),
            event_data: { topic: ['test'], value: {} },
            test_delivery: true,
        },
        null,
        2
    );
}

export async function testWebhook(): Promise<void> {
    const url = await vscode.window.showInputBox({
        prompt: 'Webhook URL to send a test event to',
        placeHolder: 'https://example.com/webhooks/pulse',
        ignoreFocusOut: true,
        validateInput: (v) => {
            try {
                new URL(v);
                return null;
            } catch {
                return 'Enter a valid absolute URL';
            }
        },
    });
    if (!url) { return; }

    const contractId = await vscode.window.showInputBox({
        prompt: 'Contract ID to embed in the sample event (optional)',
        placeHolder: 'CABC... (leave blank to use a placeholder)',
        ignoreFocusOut: true,
    });

    const timeoutMs = vscode.workspace.getConfiguration('sorobanpulse').get<number>('timeoutMs', 10000);
    const body = samplePayload(contractId?.trim() || 'CTESTCONTRACTID0000000000000000000000000000000000000');

    await vscode.window.withProgress(
        { location: vscode.ProgressLocation.Notification, title: `Sending test webhook to ${url}` },
        async () => {
            const start = Date.now();
            try {
                const result = await makeRequest(
                    {
                        method: 'POST',
                        url,
                        headers: {
                            'content-type': 'application/json',
                            'x-soroban-pulse-test': 'true',
                        },
                        body,
                    },
                    timeoutMs
                );
                const durationMs = Date.now() - start;
                const ok = result.status >= 200 && result.status < 300;

                const channel = getOutputChannel();
                channel.appendLine(`\n[${new Date().toISOString()}] POST ${url}`);
                channel.appendLine(`Status: ${result.status} ${result.statusText} (${durationMs}ms)`);
                channel.appendLine(`Body: ${truncate(result.body, 1000)}`);
                channel.show(true);

                if (ok) {
                    vscode.window.showInformationMessage(`Webhook test succeeded: ${result.status} in ${durationMs}ms.`);
                } else {
                    vscode.window.showWarningMessage(`Webhook test returned ${result.status} — see "Soroban Pulse" output for details.`);
                }
            } catch (err) {
                const message = err instanceof Error ? err.message : String(err);
                vscode.window.showErrorMessage(`Webhook test failed: ${message}`);
            }
        }
    );
}

function truncate(s: string, max: number): string {
    return s.length > max ? `${s.slice(0, max)}…` : s;
}

let outputChannel: vscode.OutputChannel | undefined;
function getOutputChannel(): vscode.OutputChannel {
    if (!outputChannel) {
        outputChannel = vscode.window.createOutputChannel('Soroban Pulse');
    }
    return outputChannel;
}
