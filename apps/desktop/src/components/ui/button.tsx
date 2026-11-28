import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";
import { cn } from "../../lib/utils";

const buttonVariants = cva(
  "focus-visible:ring-accent/40 inline-flex items-center justify-center rounded-md font-medium whitespace-nowrap transition-colors duration-[120ms] select-none focus-visible:ring-2 focus-visible:outline-none disabled:pointer-events-none disabled:opacity-40",
  {
    variants: {
      variant: {
        default: "bg-accent text-accent-ink font-medium hover:brightness-105",
        secondary: "bg-obsidian text-mist hover:text-paper hover:bg-elevated",
        ghost: "text-fog hover:text-mist hover:bg-obsidian/60",
        outline: "text-mist hover:text-paper hover:bg-obsidian bg-transparent",
        destructive: "bg-coral-red text-paper hover:brightness-110",
        pill: "bg-obsidian text-mist hover:text-paper hover:bg-graphite rounded-full",
      },
      size: {
        default: "h-9 px-4 text-sm",
        sm: "h-8 px-3 text-sm",
        lg: "h-10 px-5 text-sm",
        icon: "h-9 w-9",
        "icon-sm": "h-7 w-7",
      },
    },
    defaultVariants: {
      variant: "secondary",
      size: "default",
    },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp className={cn(buttonVariants({ variant, size, className }))} ref={ref} {...props} />
    );
  },
);
Button.displayName = "Button";

export { Button, buttonVariants };
