import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function TelegramChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">Create a bot with @BotFather, add it to the target chat, and use the chat's numeric ID (group IDs start with -100).</p>
      <SchemaFields fields={SCHEMAS.telegram.fields} {...props} />
    </>
  );
}
