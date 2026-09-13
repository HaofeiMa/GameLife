import { cn } from "../../lib/utils";

export interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  id?: string;
  "aria-label"?: string;
  className?: string;
}

/** Green when on: "enabled" is a different idea from the blue primary action. */
export function Switch({
  checked,
  onCheckedChange,
  disabled,
  id,
  className,
  ...aria
}: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      id={id}
      aria-checked={checked}
      aria-label={aria["aria-label"]}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        // The mockup's .sw: 44x26 with a 20px thumb inset 3px.
        "relative inline-flex h-[26px] w-11 shrink-0 cursor-pointer items-center rounded-full transition-colors",
        "disabled:cursor-not-allowed disabled:opacity-50",
        checked ? "bg-success" : "bg-switch-off",
        className,
      )}
    >
      <span
        className={cn(
          "pointer-events-none absolute left-[3px] block size-5 rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.2)] transition-[left] duration-200",
          checked ? "left-[21px]" : "left-[3px]",
        )}
      />
    </button>
  );
}
