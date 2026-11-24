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

  return (
    <section aria-label="Reglas por tecnología">
      <h3 className="text-xs text-fog px-3 mb-1">Reglas · {rules.length}</h3>

      {error && (
        <div
          className="mx-3 mb-2 px-3 py-2 rounded-md bg-coral-red/10 text-coral-red text-sm"
          role="alert"
        >
          {error}
        </div>
      )}

      <ul>
        {rules.map((rule) => (
          <li
            key={rule.id}
            className="flex items-start gap-3 px-3 py-2 rounded-md hover:bg-obsidian transition-colors duration-[120ms]"
          >
            <span className="text-sm font-medium text-paper whitespace-nowrap">
              {rule.technology}
              {rule.version_req && (
                <span className="text-fog tabular-nums font-normal"> {rule.version_req}</span>
              )}
            </span>
            <span className="flex-1 text-sm text-mist leading-relaxed">{rule.rule}</span>
          </li>
        ))}
        {rules.length === 0 && (
          <li className="px-3 py-3 text-xs text-fog">
            Pídele a tu IA: «añade una regla para &lt;tecnología&gt;»
          </li>
        )}
      </ul>
    </section>
  );
}
