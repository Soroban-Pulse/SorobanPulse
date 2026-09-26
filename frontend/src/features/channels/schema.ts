// Declarative config schemas for each notification channel type. Keys match the
// config structs on the server (src/slack.rs, src/discord.rs, src/telegram.rs,
// src/email.rs, src/sms.rs, src/pagerduty.rs, src/github.rs).

import type { ChannelType } from '../../api/types';

export type FieldKind = 'text' | 'secret' | 'url' | 'textarea' | 'number' | 'boolean' | 'select' | 'list';

export interface FieldSchema {
  key: string;
  label: string;
  kind: FieldKind;
  required?: boolean;
  placeholder?: string;
  help?: string;
  options?: { value: string; label: string }[];
  /** For `list` fields: regex each item must match. */
  itemPattern?: RegExp;
  pattern?: RegExp;
  patternMessage?: string;
  defaultValue?: unknown;
}

export interface ChannelSchema {
  type: ChannelType;
  label: string;
  fields: FieldSchema[];
  /** Cross-field validation beyond per-field rules. */
  validate?: (config: Record<string, unknown>) => Record<string, string>;
}

const E164 = /^\+[1-9]\d{6,14}$/;
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const HTTPS = /^https:\/\//;

export const SCHEMAS: Record<ChannelType, ChannelSchema> = {
  slack: {
    type: 'slack',
    label: 'Slack',
    fields: [
      { key: 'webhook_url', label: 'Incoming webhook URL', kind: 'secret', placeholder: 'https://hooks.slack.com/services/…', pattern: /^https:\/\/hooks\.slack\.com\//, patternMessage: 'Must be a hooks.slack.com URL' },
      { key: 'bot_token', label: 'Bot token', kind: 'secret', placeholder: 'xoxb-…', pattern: /^xox[bp]-/, patternMessage: 'Bot tokens start with xoxb- or xoxp-', help: 'Needed for threads and mentions. Provide a webhook URL, a bot token, or both.' },
      { key: 'channel', label: 'Channel', kind: 'text', required: true, placeholder: '#alerts', pattern: /^[#@]?[\w-]+$/, patternMessage: 'Use #channel, @user or a channel ID' },
      { key: 'block_kit_enabled', label: 'Use Block Kit formatting', kind: 'boolean', defaultValue: true },
      { key: 'thread_support', label: 'Group related events into threads', kind: 'boolean' },
      { key: 'user_mentions_enabled', label: 'Allow @mentions', kind: 'boolean' },
    ],
    validate: (c): Record<string, string> =>
      !c.webhook_url && !c.bot_token ? { webhook_url: 'Provide a webhook URL or a bot token' } : {},
  },
  discord: {
    type: 'discord',
    label: 'Discord',
    fields: [
      { key: 'webhook_url', label: 'Webhook URL', kind: 'secret', required: true, placeholder: 'https://discord.com/api/webhooks/…', pattern: /^https:\/\/(discord|discordapp)\.com\/api\/webhooks\//, patternMessage: 'Must be a discord.com/api/webhooks URL' },
      { key: 'bot_name', label: 'Bot display name', kind: 'text', placeholder: 'Soroban Pulse' },
      { key: 'avatar_url', label: 'Avatar URL', kind: 'url', pattern: HTTPS, patternMessage: 'Must be an https URL' },
      { key: 'embed_enabled', label: 'Send rich embeds', kind: 'boolean', defaultValue: true },
      { key: 'thread_support', label: 'Post into threads', kind: 'boolean' },
    ],
  },
  telegram: {
    type: 'telegram',
    label: 'Telegram',
    fields: [
      { key: 'bot_token', label: 'Bot token', kind: 'secret', required: true, placeholder: '123456:ABC-DEF…', pattern: /^\d+:[\w-]{30,}$/, patternMessage: 'Expected <bot id>:<token> from @BotFather' },
      { key: 'chat_id', label: 'Chat ID', kind: 'text', required: true, placeholder: '-1001234567890 or @channel', pattern: /^(-?\d+|@\w{5,})$/, patternMessage: 'Numeric chat ID or @channelname' },
      { key: 'message_thread_support', label: 'Post into forum topics', kind: 'boolean' },
      { key: 'webhook_enabled', label: 'Receive inline button callbacks', kind: 'boolean' },
      { key: 'webhook_url', label: 'Callback webhook URL', kind: 'url', pattern: HTTPS, patternMessage: 'Must be an https URL' },
    ],
  },
  email: {
    type: 'email',
    label: 'Email',
    fields: [
      { key: 'to', label: 'Recipients', kind: 'list', required: true, placeholder: 'ops@example.com, oncall@example.com', itemPattern: EMAIL, patternMessage: 'Every recipient must be an email address' },
      { key: 'from', label: 'From address', kind: 'text', placeholder: 'alerts@example.com', pattern: EMAIL, patternMessage: 'Must be an email address', help: 'Defaults to the server SMTP_FROM when empty.' },
      { key: 'subject_prefix', label: 'Subject prefix', kind: 'text', placeholder: '[Soroban Pulse]' },
      { key: 'format', label: 'Format', kind: 'select', defaultValue: 'html', options: [{ value: 'html', label: 'HTML' }, { value: 'text', label: 'Plain text' }] },
      { key: 'digest', label: 'Send as a periodic digest', kind: 'boolean' },
    ],
  },
  sms: {
    type: 'sms',
    label: 'SMS',
    fields: [
      { key: 'account_sid', label: 'Twilio account SID', kind: 'text', required: true, placeholder: 'AC…', pattern: /^AC[0-9a-fA-F]{32}$/, patternMessage: 'Starts with AC followed by 32 hex characters' },
      { key: 'auth_token', label: 'Twilio auth token', kind: 'secret', required: true },
      { key: 'from_number', label: 'From number', kind: 'text', required: true, placeholder: '+15551234567', pattern: E164, patternMessage: 'Use E.164 format, e.g. +15551234567' },
      { key: 'to_numbers', label: 'To numbers', kind: 'list', required: true, placeholder: '+15557654321, +447700900123', itemPattern: E164, patternMessage: 'Every number must be E.164, e.g. +15551234567' },
    ],
  },
  pagerduty: {
    type: 'pagerduty',
    label: 'PagerDuty',
    fields: [
      { key: 'routing_key', label: 'Integration (routing) key', kind: 'secret', required: true, pattern: /^[a-zA-Z0-9]{32}$/, patternMessage: 'Events API v2 keys are 32 alphanumeric characters' },
      { key: 'default_severity', label: 'Default severity', kind: 'select', defaultValue: 'warning', options: ['critical', 'error', 'warning', 'info'].map((v) => ({ value: v, label: v })) },
      { key: 'source', label: 'Source', kind: 'text', placeholder: 'soroban-pulse' },
      { key: 'auto_resolve', label: 'Auto-resolve when the condition clears', kind: 'boolean' },
    ],
  },
  github: {
    type: 'github',
    label: 'GitHub',
    fields: [
      { key: 'access_token', label: 'Access token', kind: 'secret', required: true, placeholder: 'ghp_… or github_pat_…', pattern: /^(gh[pousr]_|github_pat_)/, patternMessage: 'Expected a GitHub personal access or app token' },
      { key: 'owner', label: 'Owner', kind: 'text', required: true, placeholder: 'my-org', pattern: /^[\w.-]+$/, patternMessage: 'Letters, digits, dot, dash or underscore' },
      { key: 'repository', label: 'Repository', kind: 'text', required: true, placeholder: 'my-repo', pattern: /^[\w.-]+$/, patternMessage: 'Letters, digits, dot, dash or underscore' },
      { key: 'auto_create_issues', label: 'Open an issue per event', kind: 'boolean', defaultValue: true },
      { key: 'issue_title_template', label: 'Issue title template', kind: 'text', placeholder: '{{event_type}} on {{contract_id}}' },
      { key: 'issue_body_template', label: 'Issue body template', kind: 'textarea' },
      { key: 'pr_comment_enabled', label: 'Comment on linked pull requests', kind: 'boolean' },
    ],
  },
  webhook: {
    type: 'webhook',
    label: 'Webhook',
    fields: [
      { key: 'url', label: 'URL', kind: 'url', required: true, placeholder: 'https://example.com/hooks/soroban', pattern: HTTPS, patternMessage: 'Must be an https URL' },
      { key: 'secret', label: 'Signing secret', kind: 'secret' },
    ],
  },
};

export const CHANNEL_TYPES = Object.keys(SCHEMAS) as ChannelType[];

export function defaultConfig(type: ChannelType): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const f of SCHEMAS[type].fields) {
    if (f.defaultValue !== undefined) out[f.key] = f.defaultValue;
    else if (f.kind === 'boolean') out[f.key] = false;
    else if (f.kind === 'list') out[f.key] = [];
  }
  return out;
}

function isEmpty(v: unknown): boolean {
  return v === undefined || v === null || v === '' || (Array.isArray(v) && v.length === 0);
}

/**
 * Validate config against its schema. On edit, secret fields may be left blank
 * to keep the stored value, so `required` is relaxed for them.
 */
export function validateConfig(
  type: ChannelType,
  config: Record<string, unknown>,
  opts: { editing: boolean },
): Record<string, string> {
  const schema = SCHEMAS[type];
  const errors: Record<string, string> = {};
  for (const f of schema.fields) {
    const v = config[f.key];
    if (isEmpty(v)) {
      if (f.required && !(opts.editing && f.kind === 'secret')) errors[f.key] = `${f.label} is required`;
      continue;
    }
    if (f.kind === 'list' && f.itemPattern && Array.isArray(v)) {
      const bad = v.filter((item) => !f.itemPattern!.test(String(item)));
      if (bad.length) errors[f.key] = `${f.patternMessage ?? 'Invalid entry'}: ${bad.join(', ')}`;
    } else if (f.pattern && typeof v === 'string' && !f.pattern.test(v)) {
      errors[f.key] = f.patternMessage ?? `${f.label} is invalid`;
    }
  }
  return { ...(schema.validate?.(config) ?? {}), ...errors };
}
