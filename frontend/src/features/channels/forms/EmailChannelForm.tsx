import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function EmailChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">Delivered through the server's SMTP configuration. Bounced addresses are suppressed automatically.</p>
      <SchemaFields fields={SCHEMAS.email.fields} {...props} />
    </>
  );
}
