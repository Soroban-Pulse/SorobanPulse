import { ReactNode } from "react";

interface OnboardingTipsProps {
  step: number;
}

const TIPS: Record<number, string> = {
  1: "If you don't have a server yet, deploy one with `make docker-up`.",
  2: "Public network is for mainnet; Testnet is for development.",
  3: "Find contract IDs in the Stellar Explorer or your deployment scripts.",
  4: "Events may take a few seconds to appear after they are emitted on-chain.",
  5: "Alerts can be sent to Slack, Discord, or any webhook URL.",
};

export function OnboardingTips({ step }: OnboardingTipsProps) {
  return (
    <div className="onboarding-tip">
      <span className="onboarding-tip-icon">💡</span>
      <span>{TIPS[step]}</span>
    </div>
  );
}