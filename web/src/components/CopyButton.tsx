import { useState } from "react";

interface CopyButtonProps {
  value: string;
  label?: string;
}

/**
 * Copies `value` to the clipboard and shows a brief "Copied!" confirmation.
 */
export function CopyButton({ value, label }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);

  function handleCopy() {
    navigator.clipboard.writeText(value).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  return (
    <button
      className={`copy-btn${copied ? " copy-btn--done" : ""}`}
      onClick={handleCopy}
      title={label ? `Copy ${label}` : "Copy"}
      aria-label={copied ? "Copied!" : (label ? `Copy ${label}` : "Copy")}
    >
      {copied ? "✓" : "⎘"}
    </button>
  );
}
