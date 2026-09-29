// Decorative icons. Always pair with a visible label or an aria-label on the
// surrounding control; the SVGs themselves are hidden from assistive tech.

export function MenuIcon() {
  return (
    <svg aria-hidden="true" focusable="false" width="24" height="24" viewBox="0 0 24 24">
      <path d="M3 6h18M3 12h18M3 18h18" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function CloseIcon() {
  return (
    <svg aria-hidden="true" focusable="false" width="24" height="24" viewBox="0 0 24 24">
      <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function IconEmpty() {
  return (
    <svg width="64" height="64" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="8" y="12" width="48" height="40" rx="4" stroke="#888" strokeWidth="2" fill="none" />
      <line x1="8" y1="20" x2="56" y2="20" stroke="#888" strokeWidth="2" />
      <line x1="20" y1="28" x2="44" y2="28" stroke="#aaa" strokeWidth="2" strokeLinecap="round" />
      <line x1="20" y1="34" x2="40" y2="34" stroke="#aaa" strokeWidth="2" strokeLinecap="round" />
      <line x1="20" y1="40" x2="36" y2="40" stroke="#aaa" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function IconError() {
  return (
    <svg width="64" height="64" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="32" cy="32" r="28" stroke="#d64545" strokeWidth="2" fill="none" />
      <line x1="20" y1="20" x2="44" y2="44" stroke="#d64545" strokeWidth="2" strokeLinecap="round" />
      <line x1="44" y1="20" x2="20" y2="44" stroke="#d64545" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function IconWarning() {
  return (
    <svg width="64" height="64" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M32 8L56 52H8L32 8Z" stroke="#e0725b" strokeWidth="2" fill="none" />
      <line x1="32" y1="24" x2="32" y2="36" stroke="#e0725b" strokeWidth="2" strokeLinecap="round" />
      <circle cx="32" cy="42" r="2" fill="#e0725b" />
    </svg>
  );
}

export function IconLoading() {
  return (
    <svg width="64" height="64" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="32" cy="32" r="28" stroke="#e0e0e0" strokeWidth="4" fill="none" />
      <path d="M32 4C32 4 44 12 44 32C44 52 32 60 32 60" stroke="#5b8def" strokeWidth="4" strokeLinecap="round" />
    </svg>
  );
}

export function IconClock() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <circle cx="12" cy="12" r="10" stroke="#888" strokeWidth="2" fill="none" />
      <path d="M12 6V12L16 14" stroke="#888" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function IconFilter() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M4 6H20M7 12H17M10 18H14" stroke="#888" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function IconSubscriptions() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="3" y="5" width="18" height="14" rx="2" stroke="#888" strokeWidth="2" fill="none" />
      <path d="M3 9H21" stroke="#888" strokeWidth="2" />
      <circle cx="12" cy="12" r="2" fill="#888" />
    </svg>
  );
}

export function IconServer() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="2" y="4" width="20" height="16" rx="2" stroke="#888" strokeWidth="2" fill="none" />
      <line x1="2" y1="10" x2="22" y2="10" stroke="#888" strokeWidth="2" />
      <circle cx="6" cy="7" r="1" fill="#888" />
      <circle cx="6" cy="13" r="1" fill="#888" />
    </svg>
  );
}

export function IconLock() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <rect x="4" y="10" width="16" height="12" rx="2" stroke="#888" strokeWidth="2" fill="none" />
      <path d="M8 10V7C8 4.79 9.79 3 12 3C14.21 3 16 4.79 16 7V10" stroke="#888" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export function IconRetry() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M4 12C4 7.58172 7.58172 4 12 4C16.4183 4 20 7.58172 20 12" stroke="#5b8def" strokeWidth="2" strokeLinecap="round" />
      <path d="M20 12C20 16.4183 16.4183 20 12 20C7.58172 20 4 16.4183 4 12" stroke="#5b8def" strokeWidth="2" strokeLinecap="round" />
      <path d="M15 3L15 7H11" stroke="#5b8def" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M9 21L9 17H13" stroke="#5b8def" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function IconBolt() {
  return (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M13 2L7 12H11L10 22L17 10H13L14 2L13 2Z" stroke="#e0725b" strokeWidth="2" fill="none" strokeLinejoin="round" />
    </svg>
  );
}
