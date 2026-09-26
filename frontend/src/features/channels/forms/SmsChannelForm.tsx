import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function SmsChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">Sent through Twilio. Each recipient is billed per message; see the cost tracking dashboard for spend.</p>
      <SchemaFields fields={SCHEMAS.sms.fields} {...props} />
    </>
  );
}
