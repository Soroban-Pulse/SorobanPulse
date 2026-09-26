import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function SlackChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">Create an incoming webhook in your Slack app (Features → Incoming Webhooks), or install the app and paste its bot token to enable threads and mentions.</p>
      <SchemaFields fields={SCHEMAS.slack.fields} {...props} />
    </>
  );
}
