import codexbarIcon from "../assets/codexbar-icon.png";

interface TokenBarIconProps {
  size?: number;
  className?: string;
}

/**
 * Shared TokenBar brand mark in the tray / pop-out provider switcher.
 * Uses the same product PNG as the app / tray shell icon so the "全部"
 * tile matches the installed mark (not a second hand-drawn three-rail SVG).
 */
export function TokenBarIcon({ size = 18, className }: TokenBarIconProps) {
  return (
    <img
      src={codexbarIcon}
      width={size}
      height={size}
      className={className}
      alt=""
      aria-hidden
      draggable={false}
    />
  );
}
