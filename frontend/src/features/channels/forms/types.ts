export interface ChannelFormProps {
  value: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
  errors: Record<string, string>;
  editing: boolean;
}
