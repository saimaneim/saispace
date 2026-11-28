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
      <h3 className="text-fog px-1 text-xs font-semibold tabular-nums">Reglas · {rules.length}</h3>

      {error && (
        <div
          className="bg-coral-red/10 text-coral-red mb-2 rounded-md px-3 py-2 text-sm"
          role="alert"
        >
          {error}
        </div>
      )}

      {rules.length > 0 && (
        <ul className="bg-carbon space-y-0.5 rounded-xl p-1.5">
          {rules.map((rule) => (
            <li
              key={rule.id}
              className="hover:bg-obsidian flex min-w-0 items-start gap-3 rounded-lg px-3 py-2.5 transition-colors duration-[120ms]"
            >
              <span className="bg-obsidian text-paper inline-flex flex-shrink-0 items-center gap-1.5 rounded-md px-2 py-0.5 text-xs font-medium whitespace-nowrap">
                <span>{rule.technology}</span>
                {rule.version_req && (
                  <span className="text-fog font-mono font-normal tabular-nums">
                    {rule.version_req}
                  </span>
                )}
              </span>
              <span className="text-mist min-w-0 flex-1 text-sm leading-relaxed break-words">
                {rule.rule}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
