import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function WebhookChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">Soroban Pulse POSTs JSON to this URL. When a signing secret is set, requests carry an HMAC-SHA256 signature header.</p>
      <SchemaFields fields={SCHEMAS.webhook.fields} {...props} />
    </>
  );
}
