import type * as React from "react";
import { cn } from "../../lib/utils";

export function Card({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn(
        // The mockup's .card: no border, 18px, one very soft warm shadow.
        "rounded-xl bg-card text-card-foreground shadow-[0_10px_26px_-22px_rgba(120,95,60,0.7)]",
        className,
      )}
      {...props}
    />
  );
}

export function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("flex flex-col gap-1 px-5 pt-4 pb-3", className)}
      {...props}
    />
  );
}

export function CardTitle({ className, ...props }: React.ComponentProps<"h3">) {
  return (
    <h3
      className={cn("text-sm font-medium leading-none tracking-tight", className)}
      {...props}
    />
  );
}

export function CardDescription({ className, ...props }: React.ComponentProps<"p">) {
  return (
    <p
      className={cn("text-xs leading-relaxed text-muted-foreground", className)}
      {...props}
    />
  );
}

export function CardContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("px-5 pb-4", className)} {...props} />;
}

export function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("flex items-center gap-2 border-t px-5 py-3", className)}
      {...props}
    />
  );
}

/**
 * The mockup's .ch — a card's header row: title, then an optional note
 * pushed to the right edge.
 */
export function CardCh({
  title,
  meta,
}: {
  title: string;
  meta?: React.ReactNode;
}) {
  return (
    <div className="flex items-baseline gap-[9px] px-[18px] pt-3 pb-[9px]">
      <h2 className="text-[13.5px] font-semibold tracking-[-0.005em]">
        {title}
      </h2>
      {meta != null && (
        <span className="ml-auto whitespace-nowrap text-[11px] text-muted-foreground">
          {meta}
        </span>
      )}
    </div>
  );
}
