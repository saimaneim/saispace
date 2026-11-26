pub(crate) const INSTRUCTIONS: &str = concat!(
    "saispace: memory + project context shared by agents, the `sai` CLI and desktop app.\n",
    "Project = cwd's git root; first save auto-registers it.\n",
    "Start from the memory index (or memory_context); open items with get_memory (8-digit ids work) and follow their links before searching. Before asking the user to repeat anything, search_memory.\n",
    "save_memory: only durable, costly-to-rebuild knowledge (decisions + why, non-obvious root causes, constraints, pending work) as What/Why/Where/Learned. Never secrets, raw logs or routine progress.\n",
    "Save lessons at once: user rule/correction -> kind preference; own non-obvious mistake -> kind bugfix with its cause.\n",
    "Same project + topic_key updates: prefer it to new memories. Duplicates or consolidate pairs: consolidate_memories. Ids/topic_keys in content become links. Cross-project: global: true.\n",
    "After compaction or big work: session_summary (Goal, Discoveries, Accomplished, Next steps).\n",
    "Secrets: list_secrets/request_secret_access; never print values."
);
