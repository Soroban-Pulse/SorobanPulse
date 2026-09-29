import { SchemaFields } from '../SchemaFields';
import { SCHEMAS } from '../schema';
import type { ChannelFormProps } from './types';

export function GitHubChannelForm(props: ChannelFormProps) {
  return (
    <>
      <p className="help">The token needs the <code>issues: write</code> permission on the repository (and <code>pull_requests: write</code> for PR comments).</p>
      <SchemaFields fields={SCHEMAS.github.fields} {...props} />
    </>
  );
}
