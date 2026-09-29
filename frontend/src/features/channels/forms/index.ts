import type { ComponentType } from 'react';
import type { ChannelType } from '../../../api/types';
import type { ChannelFormProps } from './types';
import { DiscordChannelForm } from './DiscordChannelForm';
import { EmailChannelForm } from './EmailChannelForm';
import { GitHubChannelForm } from './GitHubChannelForm';
import { PagerDutyChannelForm } from './PagerDutyChannelForm';
import { SlackChannelForm } from './SlackChannelForm';
import { SmsChannelForm } from './SmsChannelForm';
import { TelegramChannelForm } from './TelegramChannelForm';
import { WebhookChannelForm } from './WebhookChannelForm';

export const CHANNEL_FORMS: Record<ChannelType, ComponentType<ChannelFormProps>> = {
  slack: SlackChannelForm,
  discord: DiscordChannelForm,
  telegram: TelegramChannelForm,
  email: EmailChannelForm,
  sms: SmsChannelForm,
  pagerduty: PagerDutyChannelForm,
  github: GitHubChannelForm,
  webhook: WebhookChannelForm,
};
