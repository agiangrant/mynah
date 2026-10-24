import type { ReactNode } from "react";

/** A titled group of rows. */
export function Section({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="section">
      <div className="section-head">
        <div>
          <h2 className="section-title">{title}</h2>
          {description ? <p className="section-desc">{description}</p> : null}
        </div>
        {action ? <div className="section-action">{action}</div> : null}
      </div>
      {children}
    </section>
  );
}

/** The bordered surface that holds rows. */
export function Group({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`group ${className}`.trim()}>{children}</div>;
}

/** label + description on the left, control on the right. `stack` puts children under the label at full width. */
export function Row({
  label,
  description,
  children,
  stack = false,
  disabled = false,
}: {
  label: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  stack?: boolean;
  disabled?: boolean;
}) {
  return (
    <div className={`row ${stack ? "row-stack" : ""} ${disabled ? "row-disabled" : ""}`.trim()}>
      <div className="row-main">
        <div className="row-label">{label}</div>
        {description ? <div className="row-desc">{description}</div> : null}
      </div>
      {children !== undefined ? <div className="row-control">{children}</div> : null}
    </div>
  );
}

export function Hint({ children }: { children: ReactNode }) {
  return <p className="hint">{children}</p>;
}

export function PageHead({ title, subtitle }: { title: string; subtitle?: ReactNode }) {
  return (
    <header className="page-head">
      <h1 className="page-title">{title}</h1>
      {subtitle ? <p className="page-sub">{subtitle}</p> : null}
    </header>
  );
}

export function LoadingPage() {
  return (
    <div className="loading-page" aria-busy>
      Loading…
    </div>
  );
}
