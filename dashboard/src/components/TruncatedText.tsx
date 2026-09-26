import { useState } from "react";

interface TruncatedTextProps {
  text: string;
  maxChars?: number;
  className?: string;
}

/**
 * Displays long text (IDs, hashes) with middle-ellipsis truncation.
 * Shows the full value on hover via a tooltip and provides a copy button.
 */
export function TruncatedText({ text, maxChars = 16, className }: TruncatedTextProps) {
  const [copied, setCopied] = useState(false);

  if (text.length <= maxChars) {
    return (
      <span className={className} title={text}>
        {text}
      </span>
    );
  }

  const half = Math.floor(maxChars / 2);
  const prefix = text.slice(0, half);
  const suffix = text.slice(text.length - half);
  const truncated = `${prefix}…${suffix}`;

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // clipboard API unavailable — silently ignore
    }
  }

  return (
    <span className={className} title={text} style={{ cursor: "pointer", position: "relative" }}>
      {truncated}
      <button
        onClick={handleCopy}
        aria-label={copied ? "Copied!" : `Copy ${text}`}
        title={copied ? "Copied!" : "Copy full value"}
        style={{
          marginLeft: "0.25rem",
          background: "none",
          border: "none",
          cursor: "pointer",
          padding: "0 0.125rem",
          fontSize: "0.75rem",
          opacity: copied ? 1 : 0.5,
          color: "inherit",
        }}
      >
        {copied ? "✓" : "📋"}
      </button>
    </span>
  );
}
