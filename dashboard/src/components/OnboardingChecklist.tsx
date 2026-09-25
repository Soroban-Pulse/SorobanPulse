import { useState } from "react";

interface OnboardingStep {
  id: number;
  title: string;
  description: string;
  tip: string;
  helpUrl: string;
}

const STEPS: OnboardingStep[] = [
  {
    id: 1,
    title: "Connect Server",
    description: "Enter your SorobanPulse server URL to begin.",
    tip: "If you don't have a server yet, deploy one with `make docker-up`.",
    helpUrl: "../README.md",
  },
  {
    id: 2,
    title: "Pick Network",
    description: "Choose which Stellar network to monitor.",
    tip: "Public network is for mainnet; Testnet is for development.",
    helpUrl: "../README.md",
  },
  {
    id: 3,
    title: "Watch a Contract",
    description: "Enter a Stellar contract ID to start watching events.",
    tip: "Find contract IDs in the Stellar Explorer or your deployment scripts.",
    helpUrl: "https://stellar.expert",
  },
  {
    id: 4,
    title: "See Live Events",
    description: "Watch real-time events from your contract.",
    tip: "Events appear here as they are indexed by the server.",
    helpUrl: "../api-guide.md",
  },
  {
    id: 5,
    title: "Create First Alert",
    description: "Set up an alert to be notified of specific event types.",
    tip: "Alerts can be sent to Slack, Discord, or any webhook URL.",
    helpUrl: "../alerts.md",
  },
];

interface OnboardingChecklistProps {
  onComplete: () => void;
}

export function OnboardingChecklist({ onComplete }: OnboardingChecklistProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [completed, setCompleted] = useState<Set<number>>(new Set());
  const [serverUrl, setServerUrl] = useState("");
  const [network, setNetwork] = useState("public");
  const [contractId, setContractId] = useState("");

  const step = STEPS[currentStep];
  const progress = ((currentStep + 1) / STEPS.length) * 100;

  const handleNext = () => {
    const newCompleted = new Set(completed);
    newCompleted.add(step.id);
    setCompleted(newCompleted);

    if (currentStep < STEPS.length - 1) {
      setCurrentStep(currentStep + 1);
    } else {
      onComplete();
    }
  };

  const handleBack = () => {
    if (currentStep > 0) {
      setCurrentStep(currentStep - 1);
    }
  };

  const isValid = () => {
    switch (step.id) {
      case 1:
        return serverUrl.trim().length > 0;
      case 2:
        return network === "public" || network === "testnet";
      case 3:
        return contractId.trim().length >= 56;
      default:
        return true;
    }
  };

  return (
    <div className="onboarding-checklist">
      <div className="onboarding-progress">
        <div
          className="onboarding-progress-bar"
          style={{ width: `${progress}%` }}
        />
      </div>

      <div className="onboarding-step">
        <h2>{step.title}</h2>
        <p>{step.description}</p>

        <div className="onboarding-tip">
          <span className="onboarding-tip-icon">💡</span>
          <span>{step.tip}</span>
        </div>

        {step.id === 1 && (
          <div className="onboarding-input-group">
            <label htmlFor="server-url">Server URL</label>
            <input
              id="server-url"
              type="url"
              placeholder="http://localhost:8080"
              value={serverUrl}
              onChange={(e) => setServerUrl(e.target.value)}
            />
            <a href={step.helpUrl} target="_blank" rel="noopener noreferrer">
              {step.tip}
            </a>
          </div>
        )}

        {step.id === 2 && (
          <div className="onboarding-input-group">
            <label htmlFor="network-select">Network</label>
            <select
              id="network-select"
              value={network}
              onChange={(e) => setNetwork(e.target.value)}
            >
              <option value="public">Public (Mainnet)</option>
              <option value="testnet">Testnet</option>
            </select>
          </div>
        )}

        {step.id === 3 && (
          <div className="onboarding-input-group">
            <label htmlFor="contract-id">Contract ID</label>
            <input
              id="contract-id"
              type="text"
              placeholder="CA3D5K..."
              value={contractId}
              onChange={(e) => setContractId(e.target.value)}
              maxLength={56}
            />
            <a href={step.helpUrl} target="_blank" rel="noopener noreferrer">
              Find contract IDs in Stellar Explorer
            </a>
          </div>
        )}

        {step.id === 4 && (
          <div className="onboarding-events">
            <p className="onboarding-empty-state">
              No events yet — this is normal. Events appear here once they are
              indexed by the server.
            </p>
          </div>
        )}

        {step.id === 5 && (
          <div className="onboarding-alert">
            <p>
              Create an alert to get notified when specific event types are
              detected.
            </p>
            <div className="onboarding-alert-options">
              <label htmlFor="alert-event-type">Event Type</label>
              <input
                id="alert-event-type"
                type="text"
                placeholder="e.g. transfer, mint, burn"
              />
              <label htmlFor="alert-delivery">Delivery Method</label>
              <select id="alert-delivery">
                <option value="slack">Slack</option>
                <option value="discord">Discord</option>
                <option value="webhook">Webhook</option>
              </select>
              <label htmlFor="alert-webhook">Webhook URL</label>
              <input
                id="alert-webhook"
                type="url"
                placeholder="https://your-webhook.example.com"
              />
            </div>
          </div>
        )}

        <div className="onboarding-actions">
          {currentStep > 0 && (
            <button
              className="btn btn-secondary"
              onClick={handleBack}
              type="button"
            >
              Back
            </button>
          )}
          <button
            className="btn btn-primary"
            onClick={handleNext}
            disabled={!isValid()}
            type="button"
          >
            {currentStep === STEPS.length - 1 ? "Finish Setup" : "Continue"}
          </button>
        </div>

        <div className="onboarding-step-indicators">
          {STEPS.map((s) => (
            <span
              key={s.id}
              className={`onboarding-dot ${
                completed.has(s.id)
                  ? "completed"
                  : s.id === step.id
                    ? "active"
                    : ""
              }`}
            >
              {completed.has(s.id) ? "✓" : s.id}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}