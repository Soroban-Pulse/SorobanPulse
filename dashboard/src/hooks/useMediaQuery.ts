import { useEffect, useState } from "react";

/** Matches the CSS breakpoint used for the mobile layout in styles.css. */
export const MOBILE_QUERY = "(max-width: 767.98px)";

export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);

  useEffect(() => {
    const mql = window.matchMedia(query);
    const onChange = () => setMatches(mql.matches);
    onChange();
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [query]);

  return matches;
}
