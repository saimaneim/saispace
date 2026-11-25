pub(crate) const INSTRUCTIONS: &str = concat!(
    "saispace: persistent memory + project context shared by agents, the `sai` CLI and desktop app.\n",
    "Project = working directory's git root; first save auto-registers it.\n",
    "At start you may have the memory index; open items with get_memory. Before asking the user to repeat anything, search_memory.\n",
    "Save only durable, costly-to-rebuild knowledge: decisions + why, non-obvious root causes, real constraints, pending work. Structure What/Why/Where/Learned. Never save secrets, raw logs or routine progress.\n",
    "For evolving topics, reuse topic_key: same key updates, not duplicates. If save_memory returns similar memories, update the existing one via its topic_key or delete the extra.\n",
    "User rules/preferences: kind: preference. Cross-project knowledge: global: true.\n",
    "After compaction or significant work, save session_summary (Goal, Discoveries, Accomplished, Next steps).\n",
    "Secrets: list_secrets/request_secret_access; never print values."
);
