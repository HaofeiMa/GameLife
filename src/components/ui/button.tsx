import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "../../lib/utils";

const buttonVariants = cva(
  // The mockup's .btn / .btn.pri / .btn.warm / .iconbtn. Flat surfaces, a
  // warm hairline, and no gradients or inner rings.
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-[11px] transition-colors disabled:pointer-events-none disabled:opacity-50 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        primary:
          "border border-primary bg-primary font-semibold text-primary-foreground hover:brightness-[1.05]",
        destructive:
          "border border-destructive bg-destructive font-semibold text-destructive-foreground hover:brightness-[1.05]",
        outline: "border border-btn-line bg-card text-btn-ink hover:bg-accent",
        /** .btn.warm — the quiet destructive, used for 结束今天. */
        warm: "border border-btn-line bg-card text-warm hover:bg-accent",
        secondary: "border border-btn-line bg-secondary text-secondary-foreground",
        ghost: "text-ink-dim hover:bg-accent hover:text-foreground",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        sm: "px-[10px] py-1 text-[13.5px]",
        default: "px-[13px] py-1.5 text-[14.5px]",
        lg: "px-4 py-2 text-[15px]",
        icon: "size-[30px] rounded-[10px] text-[17px]",
        "icon-sm": "size-[26px] rounded-[9px]",
      },
    },
    defaultVariants: { variant: "primary", size: "default" },
  },
);

export interface ButtonProps
  extends React.ComponentProps<"button">,
    VariantProps<typeof buttonVariants> {}

export function Button({
  className,
  variant,
  size,
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  );
}

export { buttonVariants };
