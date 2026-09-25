# First-Run Onboarding Flow (Issue #1117)

## Journey Map

A first-time user lands on an empty dashboard with no server configured, no events and no subscriptions. The path to "aha" (seeing live events from a contract they care about) is:

1. **Connect Server** — The user provides the SorobanPulse server URL
2. **Pick Network** — The user selects which Stellar network to monitor (Public, Testnet, or Custom)
3. **Watch a Contract** — The user enters a contract ID to start watching
4. **See Live Events** — The dashboard shows real-time events from the watched contract
5. **Create First Alert** — The user sets up an alert for specific event types

## Onboarding Checklist Component

### Design

The onboarding checklist appears as a sidebar or top-banner on the StatusDashboard when the user has no subscriptions or servers configured. It uses a step-by-step progress indicator with contextual tips.

### Component: `OnboardingChecklist`

Location: `dashboard/src/components/OnboardingChecklist.tsx`

The component renders:
- A progress bar showing current step (1–5)
- A checklist of steps with completion status
- Contextual tips that change based on the current step
- A "Get Started" CTA for the first step
- Navigation buttons (Next / Back) for intermediate steps
- A "Finish Setup" CTA for the final step

### Step Details

| Step | Title | Description | Tip |
|------|-------|-------------|-----|
| 1 | Connect Server | Enter your SorobanPulse server URL to begin | "If you don't have a server yet, deploy one with `make docker-up`" |
| 2 | Pick Network | Choose which Stellar network to monitor | "Public network is for mainnet; Testnet is for development" |
| 3 | Watch a Contract | Enter a Stellar contract ID to start watching events | "Find contract IDs in the Stellar Explorer or your deployment scripts" |
| 4 | See Live Events | Watch real-time events from your contract | "Events appear here as they are indexed by the server" |
| 5 | Create First Alert | Set up an alert to be notified of specific event types | "Alerts can be sent to Slack, Discord, or any webhook URL" |

### Contextual Tips

Each step includes a contextual tip that provides helpful guidance:
- Step 1: Links to deployment docs
- Step 2: Explains network differences
- Step 3: Shows how to find contract IDs
- Step 4: Explains event indexing delay
- Step 5: Shows alert delivery options

## Backend API: `/v1/meta`

### Purpose

The `/v1/meta` endpoint provides the metadata needed to drive the onboarding flow. It returns:
- Available networks
- Server configuration status
- Whether the user has existing subscriptions
- System health

### Endpoint Specification

```
GET /api/v1/meta
```

### Response Schema

```json
{
  "serverUrl": "http://localhost:8080",
  "networks": ["public", "testnet"],
  "defaultNetwork": "public",
  "hasSubscriptions": false,
  "hasServers": false,
  "systemStatus": "healthy",
  "version": "1.0.0",
  "onboardingComplete": false
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `serverUrl` | string | The configured SorobanPulse server URL |
| `networks` | string[] | Available Stellar networks (public, testnet) |
| `defaultNetwork` | string | The default network to monitor |
| `hasSubscriptions` | boolean | Whether the user has existing subscriptions |
| `hasServers` | boolean | Whether the user has configured servers |
| `systemStatus` | "healthy" \| "degraded" \| "down" | Current system health |
| `version` | string | Server version |
| `onboardingComplete` | boolean | Whether onboarding has been completed |

## Microcopy

See `docs/onboarding-copy.md` for the complete microcopy document.

## Implementation Notes

- The onboarding flow is only shown to first-time users (no existing subscriptions)
- The checklist component is displayed on the StatusDashboard
- Each step saves progress to localStorage so the user can resume
- The `/v1/meta` endpoint is used to check server configuration status
- The onboarding is considered complete when the user has at least one active subscription