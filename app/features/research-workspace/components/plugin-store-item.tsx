"use client";

import type { ReactNode } from "react";

export interface PluginStoreItemProps {
  name: string;
  version: string;
  kind?: string;
  description: string;
  icon: ReactNode;
  status?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
  onOpenSettings?: () => void;
}

/** Shared clickable article for built-in and installed plugin entries. */
export function PluginStoreItem({
  name,
  version,
  kind,
  description,
  icon,
  status,
  actions,
  children,
  onOpenSettings,
}: PluginStoreItemProps) {
  const clickable = Boolean(onOpenSettings);

  return (
    <article
      className={`rounded-[5px] border border-ink/18 p-4 ${
        clickable
          ? "cursor-pointer transition-colors hover:border-blue/40 hover:bg-blue/[0.025] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
          : ""
      }`}
      onClick={clickable ? onOpenSettings : undefined}
      onKeyDown={
        clickable
          ? (event) => {
              if (event.target !== event.currentTarget) return;
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onOpenSettings?.();
              }
            }
          : undefined
      }
      role={clickable ? "button" : undefined}
      tabIndex={clickable ? 0 : undefined}
      aria-label={clickable ? `Open settings for ${name}` : undefined}
    >
      <div className="flex items-start gap-3">
        {icon}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h4 className="font-serif text-[13px]">{name}</h4>
            <span className="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/40">
              {kind ? `${kind} · ` : ""}
              {version}
            </span>
          </div>
          <p className="mt-1 font-serif text-[9px] leading-[1.45] text-ink/50">
            {description}
          </p>
          {children}
        </div>
        {(status || actions) && (
          <div
            className="flex shrink-0 items-start gap-2"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => event.stopPropagation()}
          >
            {status}
            {actions}
          </div>
        )}
      </div>
    </article>
  );
}
