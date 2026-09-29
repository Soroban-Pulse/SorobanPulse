import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function DiscordChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">In Discord: Server Settings → Integrations → Webhooks → New Webhook, then copy the webhook URL.</p>
      <SchemaFields fields={SCHEMAS.discord.fields} {...props} />
    </>
  );
}
