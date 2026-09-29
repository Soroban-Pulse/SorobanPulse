import * as vscode from 'vscode';
import * as https from 'https';
import * as http from 'http';
import { URL } from 'url';
import { getApiKey } from './apiKeyManager';
import { findContractId, formatEvent, isContractId, SseParser } from './eventStream';

// ---------------------------------------------------------------------------
// "Tail Contract Events" (Issue #1124)
//
// Streams `/v1/events/stream?contract_id=` into an Output channel. One tail
// runs at a time; starting another (or "Stop Tailing Events") ends it.
// ---------------------------------------------------------------------------

export class EventTail implements vscode.Disposable {
    private readonly output = vscode.window.createOutputChannel('Soroban Pulse Events');
    private request: http.ClientRequest | undefined;

    constructor(private readonly context: vscode.ExtensionContext) {}

    async start(): Promise<void> {
        const contractId = await this.promptContractId();
        if (!contractId) { return; }

        this.stop();
        const base = vscode.workspace.getConfiguration('sorobanpulse').get<string>('baseUrl', 'http://localhost:3000');
        const url = new URL(base.replace(/\/$/, '') + '/v1/events/stream');
        url.searchParams.set('contract_id', contractId);

        const headers: Record<string, string> = { Accept: 'text/event-stream' };
        const apiKey = await getApiKey(this.context);
        if (apiKey) { headers['x-api-key'] = apiKey; }

        this.output.show(true);
        this.output.appendLine(`Tailing ${contractId} from ${url.origin} …`);

        const transport = url.protocol === 'https:' ? https : http;
        const req = transport.get(url, { headers }, res => this.onResponse(req, res));
        req.on('error', err => {
            if (this.request === req) {
                this.output.appendLine(`Connection error: ${err.message}`);
                this.request = undefined;
            }
        });
        this.request = req;
    }

    stop(): void {
        if (!this.request) { return; }
        const req = this.request;
        this.request = undefined;
        req.destroy();
        this.output.appendLine('Tail stopped.');
    }

    dispose(): void {
        this.stop();
        this.output.dispose();
    }

    private onResponse(req: http.ClientRequest, res: http.IncomingMessage): void {
        res.setEncoding('utf8');
        if (res.statusCode !== 200) {
            let body = '';
            res.on('data', (chunk: string) => { body += chunk; });
            res.on('end', () => {
                this.output.appendLine(`Server responded ${res.statusCode}: ${body.trim() || res.statusMessage || ''}`);
                if (this.request === req) { this.request = undefined; }
            });
            return;
        }

        this.output.appendLine('Connected. Waiting for events…');
        const parser = new SseParser();
        res.on('data', (chunk: string) => {
            for (const message of parser.push(chunk)) {
                this.output.appendLine(formatEvent(message));
            }
        });
        res.on('end', () => {
            if (this.request === req) {
                this.output.appendLine('Stream closed by server.');
                this.request = undefined;
            }
        });
    }

    private async promptContractId(): Promise<string | undefined> {
        const editor = vscode.window.activeTextEditor;
        const selected = editor ? editor.document.getText(editor.selection) : '';
        const value = await vscode.window.showInputBox({
            prompt: 'Contract ID to tail',
            placeHolder: 'C…',
            value: findContractId(selected) ?? '',
            ignoreFocusOut: true,
            validateInput: v => isContractId(v) ? undefined : 'Enter a contract ID: C followed by 55 base32 characters.',
        });
        return value?.trim();
    }
}
