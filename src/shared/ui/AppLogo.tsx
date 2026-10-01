import yiSvg from "@/assets/providers/yi.svg?raw";

interface AppLogoProps {
  className?: string;
  alt?: string;
}

/** Inline the trusted, bundled Yi vector so currentColor follows the application theme. */
export function AppLogo({ className, alt = "" }: AppLogoProps) {
  return <span className={`app-logo${className ? ` ${className}` : ""}`}
    role={alt ? "img" : undefined} aria-label={alt || undefined} aria-hidden={alt === ""}
    dangerouslySetInnerHTML={{ __html: yiSvg }} />;
}
