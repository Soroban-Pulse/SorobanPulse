import type { Meta, StoryObj } from "@storybook/react";
import { EmptyState } from "./EmptyState";
import { LoadingState } from "./LoadingState";
import { ErrorBanner } from "./ErrorBanner";

const meta: Meta = { title: "Design System/State Patterns" };
export default meta;

export const Empty: StoryObj<typeof EmptyState> = {
  render: (args) => <EmptyState {...args} />,
  args: {
    title: "No events yet",
    description: "Events will appear here once the indexer processes a ledger.",
    actionLabel: "Add contract",
    onAction: () => {},
  },
};

export const LoadingTable: StoryObj<typeof LoadingState> = {
  render: (args) => <LoadingState {...args} />,
  args: { layout: "table", rows: 8, columns: 5 },
};

export const LoadingCards: StoryObj<typeof LoadingState> = {
  render: (args) => <LoadingState {...args} />,
  args: { layout: "cards", rows: 4 },
};

export const LoadingChart: StoryObj<typeof LoadingState> = {
  render: (args) => <LoadingState {...args} />,
  args: { layout: "chart" },
};

export const Error: StoryObj<typeof ErrorBanner> = {
  render: (args) => <ErrorBanner {...args} />,
  args: {
    title: "Couldn't load events.",
    message: "The API returned 503. Your filters are kept.",
    onRetry: () => {},
  },
};
