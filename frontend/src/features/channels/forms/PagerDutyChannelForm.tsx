import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function PagerDutyChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">Use an Events API v2 integration key from a PagerDuty service (Service → Integrations → Events API V2).</p>
      <SchemaFields fields={SCHEMAS.pagerduty.fields} {...props} />
    </>
  );
}
