import type { CSSProperties } from "react";
export function Icon({
  name,
  size = 20,
  style,
}: {
  name: string;
  size?: number;
  style?: CSSProperties;
}) {
  const paths: Record<string, React.ReactNode> = {
    wave: <path d="M3 10v4m4-8v12m5-16v20m5-16v12m4-8v4" />,
    live: (
      <>
        <rect x="3" y="4" width="18" height="14" rx="3" />
        <path d="M8 22h8m-4-4v4m-6-9h2l2-3 3 6 2-3h3" />
      </>
    ),
    history: (
      <>
        <path d="M3 11a9 9 0 1 1 2 7M3 4v7h7m2-5v6l4 2" />
      </>
    ),
    settings: (
      <>
        <path d="M4 7h16M4 17h16" />
        <circle cx="8" cy="7" r="3" />
        <circle cx="16" cy="17" r="3" />
      </>
    ),
    play: <path d="m8 5 11 7-11 7Z" />,
    pause: <path d="M8 5v14M16 5v14" />,
    stop: <rect x="6" y="6" width="12" height="12" rx="2" />,
    arrow: <path d="M4 12h16m-6-6 6 6-6 6" />,
    search: (
      <>
        <circle cx="10" cy="10" r="6" />
        <path d="m15 15 5 5" />
      </>
    ),
    check: <path d="m5 12 4 4L19 6" />,
    copy: (
      <>
        <rect x="8" y="8" width="12" height="13" rx="2" />
        <path d="M16 8V3H3v13h5" />
      </>
    ),
    download: <path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5" />,
    close: <path d="m6 6 12 12M6 18 18 6" />,
    globe: (
      <>
        <circle cx="12" cy="12" r="9" />
        <ellipse cx="12" cy="12" rx="4" ry="9" />
        <path d="M3 12h18" />
      </>
    ),
    shield: (
      <>
        <path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6Z" />
        <path d="m8 12 3 3 5-6" />
      </>
    ),
    chevron: <path d="m9 5 7 7-7 7" />,
    trash: <path d="M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7m4-7v7" />,
    volume: (
      <path d="m11 4-6 5H2v6h3l6 5ZM15 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14" />
    ),
    info: (
      <>
        <circle cx="12" cy="12" r="9" />
        <path d="M12 11v6m0-10v1" />
      </>
    ),
  };
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.65"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      style={style}
    >
      {paths[name] ?? paths.info}
    </svg>
  );
}
