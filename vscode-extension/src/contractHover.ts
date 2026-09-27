// ---------------------------------------------------------------------------
// Contract ID hover helpers (Issue #1125)
//
// `vscode`-free core of the contract hover: the ID pattern, a short-lived
// cache in front of `/v1/contracts/{id}/summary`, and Markdown rendering.
// Lookups never throw; any failure yields `undefined` so the hover stays quiet.
// ---------------------------------------------------------------------------

/** Soroban contract strkey: `C` followed by 55 base32 characters. */
export const CONTRACT_ID_PATTERN = /C[A-Z2-7]{55}/;

/** Subset of `ContractDetailSummary` the hover shows. */
export interface ContractSummary {
    contract_id: string;
    total_events: number;
    last_event_at?: string | null;
    ledger_range?: { min?: number | null; max?: number | null };
}

export type SummaryFetcher = (contractId: string) => Promise<ContractSummary | undefined>;

interface Entry {
    expires: number;
    value: Promise<ContractSummary | undefined>;
}

export class SummaryCache {
    private readonly entries = new Map<string, Entry>();

    constructor(
        private readonly fetcher: SummaryFetcher,
        private readonly ttlMs = 30_000,
        private readonly now: () => number = Date.now,
    ) {}

    /** Cached summary for `contractId`; concurrent hovers share one request. */
    get(contractId: string): Promise<ContractSummary | undefined> {
        const hit = this.entries.get(contractId);
        if (hit && hit.expires > this.now()) { return hit.value; }

        // Failures are cached too, so an unreachable server is not re-hit on every hover.
        const value = this.fetcher(contractId).catch(() => undefined);
        this.entries.set(contractId, { expires: this.now() + this.ttlMs, value });
        return value;
    }

    clear(): void {
        this.entries.clear();
    }
}

export function renderSummary(summary: ContractSummary): string {
    const lines = [
        `**Soroban Pulse** \`${summary.contract_id}\``,
        '',
        `- Total events: ${summary.total_events.toLocaleString('en-US')}`,
    ];
    const lastLedger = summary.ledger_range?.max;
    if (typeof lastLedger === 'number') { lines.push(`- Last seen ledger: ${lastLedger}`); }
    if (summary.last_event_at) { lines.push(`- Last event: ${summary.last_event_at}`); }
    return lines.join('\n');
}
