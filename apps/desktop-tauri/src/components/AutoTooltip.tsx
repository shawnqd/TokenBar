import { useState, useRef, type ReactNode, type ElementType, type ComponentPropsWithoutRef } from "react";

export interface AutoTooltipProps<T extends ElementType = "span"> {
  text: string;
  as?: T;
  className?: string;
  children?: ReactNode;
}

/**
 * AutoTooltip only enables title/tooltip when text overflows its container
 * and is truncated by ellipsis (scrollWidth > clientWidth).
 * When text fits completely without overflow, title remains undefined.
 */
export function AutoTooltip<T extends ElementType = "span">({
  text,
  as,
  className,
  children,
  ...rest
}: AutoTooltipProps<T> & Omit<ComponentPropsWithoutRef<T>, keyof AutoTooltipProps<T>>) {
  const Component = as || "span";
  const ref = useRef<HTMLElement | null>(null);
  const [tooltip, setTooltip] = useState<string | undefined>(undefined);

  const handleMouseEnter = () => {
    const el = ref.current;
    if (el && el.scrollWidth > el.clientWidth) {
      setTooltip(text);
    } else {
      setTooltip(undefined);
    }
  };

  return (
    <Component
      ref={ref}
      className={className}
      title={tooltip}
      onMouseEnter={handleMouseEnter}
      {...(rest as any)}
    >
      {children ?? text}
    </Component>
  );
}
