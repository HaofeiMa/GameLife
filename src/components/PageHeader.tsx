import type { ReactNode } from "react";

/**
 * Enough room under the macOS traffic lights that the rail's mark is not
 * covered. The design asks for no reserved space at all; this is the
 * smallest strip that keeps the window controls usable.
 */
export const TRAFFIC_LIGHT_STRIP = 28;
/**
 * The mockup's .hd height. The header sits to the right of the rail, so it
 * carries no traffic lights and keeps the design's flat 58px.
 */
export const TOOLBAR_ROW = 58;
/** Breathing room under the brand row, above the shared border. */
export const TOOLBAR_PAD_BOTTOM = 0;
/**
 * Total toolbar height. The sidebar's brand block is sized from the same
 * numbers — if they drift the sidebar's first nav item rides up above the
 * toolbar border and the two columns stop reading as one surface.
 */
export const TOOLBAR_HEIGHT =
  TRAFFIC_LIGHT_STRIP + TOOLBAR_ROW + TOOLBAR_PAD_BOTTOM;

export interface PageHeaderProps {
  /** Usually just an `<h1>`. */
  title: ReactNode;
  /** Muted text after the title, behind a hairline divider. */
  subtitle?: ReactNode;
  /** Page-level controls (segmented nav). Centered in the whole bar. */
  center?: ReactNode;
  /** Right-aligned actions. */
  actions?: ReactNode;
}

/**
 * Page toolbar. Owns its own vertical space above the scroll region rather
 * than sticking, so page content never scrolls under it.
 *
 * The bar is as tall as the sidebar's traffic-light strip plus brand row, so
 * the bottom border meets the column divider. Content is vertically centered
 * in that full height — the right column has no traffic lights, so using the
 * strip as padding-top left a blank band above the title.
 *
 * `center` is positioned against the bar itself, not the leftover gap between
 * `title` and `actions`.
 */
export function PageHeader({
  title,
  subtitle,
  center,
  actions,
}: PageHeaderProps) {
  return (
    <header
      data-tauri-drag-region="deep"
      className="relative flex shrink-0 items-center bg-background px-[22px]"
      style={{ height: TOOLBAR_HEIGHT }}
    >
      <div className="no-drag relative z-10 flex min-w-0 shrink-0 items-center">
        {title}
        {subtitle && (
          <>
            <i
              className="mx-3 h-[17px] w-px shrink-0 bg-input"
              aria-hidden
            />
            <span className="truncate text-[12.5px] text-muted-foreground">
              {subtitle}
            </span>
          </>
        )}
      </div>
      {center && (
        <div className="pointer-events-none absolute inset-0 z-0 flex items-center justify-center">
          <div className="no-drag pointer-events-auto">{center}</div>
        </div>
      )}
      {actions && (
        <div className="no-drag relative z-10 ml-auto flex shrink-0 items-center gap-2">
          {actions}
        </div>
      )}
    </header>
  );
}
