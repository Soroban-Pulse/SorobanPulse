// ---------------------------------------------------------------------------
// Live event tail helpers (Issue #1124)
//
// Pure, `vscode`-free pieces of the "Tail Contract Events" command: an
// incremental Server-Sent Events parser for `/v1/events/stream`, contract ID
// detection for pre-filling the prompt, and output formatting.
// ---------------------------------------------------------------------------

export interface SseMessage {
    id?: string;
    event: string;
    data: string;
}

/** Soroban contract strkey: `C` followed by 55 base32 characters. */
const CONTRACT_ID = /\bC[A-Z2-7]{55}\b/;

export function isContractId(value: string): boolean {
    return new RegExp(`^${CONTRACT_ID.source}$`).test(value.trim());
}

/** First contract ID found in `text` (e.g. the editor selection), if any. */
export function findContractId(text: string): string | undefined {
    return CONTRACT_ID.exec(text)?.[0];
}

/**
 * Incremental SSE parser. Feed it chunks as they arrive; it returns the
 * messages completed by that chunk and buffers any partial line or event.
 */
export class SseParser {
    private buffer = '';
    private data: string[] = [];
    private event = '';
    private id: string | undefined;

    /** Last `id:` seen on a dispatched message (for a future `Last-Event-ID`). */
    lastEventId: string | undefined;

    push(chunk: string): SseMessage[] {
        this.buffer += chunk;
        const messages: SseMessage[] = [];
        let newline: number;
        while ((newline = this.buffer.search(/\r\n|\r|\n/)) !== -1) {
            // A lone trailing `\r` may be the first half of `\r\n`; wait for more.
            if (this.buffer[newline] === '\r' && newline === this.buffer.length - 1) { break; }
            const width = this.buffer.startsWith('\r\n', newline) ? 2 : 1;
            const line = this.buffer.slice(0, newline);
            this.buffer = this.buffer.slice(newline + width);
            const message = this.line(line);
            if (message) { messages.push(message); }
        }
        return messages;
    }

    private line(line: string): SseMessage | undefined {
        if (line === '') { return this.dispatch(); }
        if (line.startsWith(':')) { return undefined; } // comment / keep-alive

        const colon = line.indexOf(':');
        const field = colon === -1 ? line : line.slice(0, colon);
        let value = colon === -1 ? '' : line.slice(colon + 1);
        if (value.startsWith(' ')) { value = value.slice(1); }

        switch (field) {
            case 'data': this.data.push(value); break;
            case 'event': this.event = value; break;
            case 'id': if (!value.includes('\0')) { this.id = value; } break;
            default: break; // `retry` and unknown fields are ignored
        }
        return undefined;
    }

    private dispatch(): SseMessage | undefined {
        const hasData = this.data.length > 0;
        const message: SseMessage = { id: this.id, event: this.event || 'message', data: this.data.join('\n') };
        if (this.id !== undefined) { this.lastEventId = this.id; }
        this.data = [];
        this.event = '';
        this.id = undefined;
        return hasData ? message : undefined;
    }
}

/** Render one streamed event for the Output channel: a header line plus pretty JSON. */
export function formatEvent(message: SseMessage, now: Date = new Date()): string {
    let body: unknown;
    try {
        body = JSON.parse(message.data);
    } catch {
        return `[${now.toISOString()}] ${message.event}\n${message.data}\n`;
    }
    const fields = typeof body === 'object' && body !== null ? body as Record<string, unknown> : {};
    const header = [
        `[${now.toISOString()}]`,
        typeof fields.event_type === 'string' ? fields.event_type : message.event,
        typeof fields.ledger === 'number' ? `ledger ${fields.ledger}` : undefined,
        typeof fields.tx_hash === 'string' ? `tx ${fields.tx_hash}` : undefined,
    ].filter(Boolean).join('  ');
    return `${header}\n${JSON.stringify(body, null, 2)}\n`;
}
