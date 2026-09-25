# Onboarding Microcopy (Issue #1117)

## Step 1: Connect Server

### Headline
Connect your SorobanPulse server

### Body
To get started, enter the URL of your SorobanPulse server. This is where your events are indexed and stored.

### Placeholder
"Enter your server URL, e.g. http://localhost:8080"

### CTA Button
Connect Server

### Help Text
Don't have a server yet? [Deploy one with Docker →](../README.md)

### Error Message
Please enter a valid URL (e.g. http://localhost:8080 or https://your-server.example.com)

---

## Step 2: Pick Network

### Headline
Choose your Stellar network

### Body
Select the Stellar network you want to monitor. Each network is independent — events from one won't appear on another.

### Network Options
- **Public** — The main Stellar network. Use this for production monitoring.
- **Testnet** — The Stellar test network. Use this for development and testing.

### CTA Button
Continue

### Help Text
Switching networks later is easy — go to Settings → Network at any time.

---

## Step 3: Watch a Contract

### Headline
Start watching a contract

### Body
Enter the Stellar contract ID you want to monitor. SorobanPulse will begin indexing events from this contract in real time.

### Placeholder
"Enter contract ID, e.g. CA3D5K... "

### CTA Button
Start Watching

### Help Text
Find contract IDs in the [Stellar Explorer](https://stellar.expert) or your deployment scripts.

### Error Message
Please enter a valid Stellar contract ID (starts with "CA" and is 56 characters long)

---

## Step 4: See Live Events

### Headline
Your events are coming in

### Body
SorobanPulse is now indexing events from your contract. Live events will appear here as they are processed.

### Empty State
No events yet — this is normal. Events appear here once they are indexed by the server.

### CTA Button
View Subscriptions

### Help Text
Events may take a few seconds to appear after they are emitted on-chain.

---

## Step 5: Create First Alert

### Headline
Set up your first alert

### Body
Stay informed about the events that matter most. Create an alert to get notified when specific event types are detected.

### Alert Options
- **Event Type** — Filter by specific event types (e.g. `transfer`, `mint`, `burn`)
- **Delivery Method** — Choose how you want to be notified (Slack, Discord, Webhook)
- **Webhook URL** — The URL where alerts will be sent

### CTA Button
Create Alert

### Help Text
You can create more alerts later from the Alerts page. Alerts are delivered in real time.

---

## Onboarding Complete

### Headline
You're all set!

### Body
Your SorobanPulse dashboard is now configured. You're watching your first contract and ready to receive events.

### CTA Button
Go to Dashboard

### Help Text
Need more help? Check out the [API Guide](../api-guide.md) or [FAQ](../FAQ.md).