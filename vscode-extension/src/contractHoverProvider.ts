import * as vscode from 'vscode';
import * as https from 'https';
import * as http from 'http';
import { URL } from 'url';
import { getApiKey } from './apiKeyManager';
import { CONTRACT_ID_PATTERN, ContractSummary, renderSummary, SummaryCache } from './contractHover';

// ---------------------------------------------------------------------------
// Contract ID hover provider (Issue #1125)
//
// Hovering a `C…` contract ID in Rust, TypeScript, JavaScript or JSON shows
// its SorobanPulse summary. Errors, timeouts and unknown contracts produce
// no hover at all.
// ---------------------------------------------------------------------------

export const HOVER_LANGUAGES: vscode.DocumentSelector = [
    { language: 'rust' },
    { language: 'typescript' },
    { language: 'typescriptreact' },
    { language: 'javascript' },
    { language: 'javascriptreact' },
    { language: 'json' },
    { language: 'jsonc' },
];

const HOVER_TIMEOUT_MS = 3000;

function fetchSummary(context: vscode.ExtensionContext): (contractId: string) => Promise<ContractSummary | undefined> {
    return async contractId => {
        const base = vscode.workspace.getConfiguration('sorobanpulse').get<string>('baseUrl', 'http://localhost:3000');
        const url = new URL(`${base.replace(/\/$/, '')}/v1/contracts/${contractId}/summary`);
        const headers: Record<string, string> = { Accept: 'application/json' };
        const apiKey = await getApiKey(context);
        if (apiKey) { headers['x-api-key'] = apiKey; }

        const transport = url.protocol === 'https:' ? https : http;
        return new Promise(resolve => {
            const req = transport.get(url, { headers, timeout: HOVER_TIMEOUT_MS }, res => {
                let body = '';
                res.setEncoding('utf8');
                res.on('data', (chunk: string) => { body += chunk; });
                res.on('end', () => {
                    if (res.statusCode !== 200) { return resolve(undefined); }
                    try {
                        const parsed = JSON.parse(body) as ContractSummary;
                        resolve(typeof parsed.total_events === 'number' ? parsed : undefined);
                    } catch {
                        resolve(undefined);
                    }
                });
            });
            req.on('timeout', () => req.destroy());
            req.on('error', () => resolve(undefined));
        });
    };
}

export class ContractHoverProvider implements vscode.HoverProvider {
    private readonly cache: SummaryCache;

    constructor(context: vscode.ExtensionContext) {
        this.cache = new SummaryCache(fetchSummary(context));
    }

    async provideHover(document: vscode.TextDocument, position: vscode.Position): Promise<vscode.Hover | undefined> {
        const range = document.getWordRangeAtPosition(position, CONTRACT_ID_PATTERN);
        if (!range) { return undefined; }
        const summary = await this.cache.get(document.getText(range));
        return summary ? new vscode.Hover(new vscode.MarkdownString(renderSummary(summary)), range) : undefined;
    }

    /** Drop cached summaries, e.g. after the base URL or API key changes. */
    clear(): void {
        this.cache.clear();
    }
}
