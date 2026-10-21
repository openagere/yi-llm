import yiSvg from "@/assets/providers/yi.svg?raw";

interface AppLogoProps {
  className?: string;
  alt?: string;
  wordmark?: boolean;
}

/**
 * Inline the bundled Yi mark + custom geometric SVG wordmark ("Token").
 * Since the Yi icon itself is the "Yi" lettermark, the wordmark renders "Token"
 * in matching rounded-stroke geometry so the lockup reads naturally as "Yi Token"
 * without repeating "Yi" or relying on default system fonts.
 */
export function AppLogo({ className, alt = "", wordmark = false }: AppLogoProps) {
  return (
    <span
      className={`app-logo${wordmark ? " app-logo-lockup" : ""}${className ? ` ${className}` : ""}`}
      role={alt ? "img" : undefined}
      aria-label={alt || undefined}
      aria-hidden={alt === ""}
    >
      <span className="app-logo-mark" dangerouslySetInnerHTML={{ __html: yiSvg }} />
      {wordmark && (
        <svg
          className="app-logo-wordmark"
          viewBox="0 0 82 24"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
          aria-hidden="true"
        >
          <g
            stroke="currentColor"
            strokeWidth="2.35"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            {/* T */}
            <path d="M2 4.6H14.6M8.3 4.6V20.2" />
            {/* o */}
            <rect x="18.2" y="9" width="10.4" height="11.2" rx="5.2" />
            {/* k */}
            <path d="M33.6 3.8V20.2M42.2 9.2L34.2 14.6M36.6 13L42.8 20.2" />
            {/* e */}
            <path d="M47.4 14.4H57.6V13.8C57.6 10.9 55.4 9 52.5 9C49.5 9 47.3 11.2 47.3 14.6C47.3 18 49.6 20.2 52.8 20.2C55 20.2 56.6 19.4 57.4 18.2" />
            {/* n */}
            <path d="M62.6 20.2V9.2M62.6 12.6C63.5 10.2 65.4 9 67.9 9C70.9 9 72.8 10.9 72.8 13.8V20.2" />
          </g>
        </svg>
      )}
    </span>
  );
}
