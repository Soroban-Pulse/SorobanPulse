import { OnboardingChecklist } from "../components/OnboardingChecklist";

export function OnboardingPage() {
  function handleComplete() {
    localStorage.setItem("sorobanpulse.onboarding.complete", "true");
    window.location.href = "/";
  }

  return (
    <div className="onboarding-page">
      <h1>Welcome to SorobanPulse</h1>
      <p>Let's get you set up in a few quick steps.</p>
      <OnboardingChecklist onComplete={handleComplete} />
    </div>
  );
}