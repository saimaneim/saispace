import { useCallback, useEffect, useState } from "react";
import { api, formatError } from "../lib/api";
import type { Technology, TechRule } from "../lib/types";

interface TechRulesSectionProps {
  projectId: string;
  stack: Technology[];
  stackVersion: number;
}

export function TechRulesSection({ projectId, stackVersion }: TechRulesSectionProps) {
  const [rules, setRules] = useState<TechRule[]>([]);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setRules(await api.listProjectTechRules(projectId));
    } catch (err) {
      setError(formatError(err));
    }
  }, [projectId]);

  useEffect(() => {
    load();
  }, [load, stackVersion]);

  if (rules.length === 0 && !error) return null;

  return (
    <section aria-label="Reglas por tecnología" className="space-y-2">
      <h3 className="text-xs font-semibold text-fog px-1 tabular-nums">Reglas · {rules.length}</h3>

      {error && (
        <div
          className="mb-2 px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm"
          role="alert"
        >
          {error}
        </div>
      )}

      {rules.length > 0 && (
        <ul className="bg-carbon rounded-xl p-1.5 space-y-0.5">
          {rules.map((rule) => (
            <li
              key={rule.id}
              className="flex items-start gap-3 px-3 py-2.5 rounded-lg hover:bg-obsidian transition-colors duration-[120ms] min-w-0"
            >
              <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-obsidian text-xs font-medium text-paper whitespace-nowrap flex-shrink-0">
                <span>{rule.technology}</span>
                {rule.version_req && (
                  <span className="text-fog tabular-nums font-mono font-normal">
                    {rule.version_req}
                  </span>
                )}
              </span>
              <span className="flex-1 min-w-0 text-sm text-mist leading-relaxed break-words">
                {rule.rule}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
