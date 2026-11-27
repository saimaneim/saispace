pub(crate) const INSTRUCTIONS: &str = concat!(
    "saispace: memory + project context shared by agents, the `sai` CLI and desktop app.\n",
    "Project = cwd's git root; first save registers it.\n",
    "Start from the memory index (or memory_context); open items with get_memory (8-digit ids ok) and follow links. Before asking the user to repeat, search_memory.\n",
    "save_memory: durable, costly knowledge (decisions + why, constraints, pending work) as What/Why/Where/Learned. Never secrets or logs.\n",
    "Save lessons at once: user correction -> kind preference; own mistake -> kind bugfix with its cause and triggers (exact error line, or env:<host>) so it returns when the error repeats. Temporary facts (quota out until X): expires_at.\n",
    "Same project + topic_key updates. Duplicates: consolidate_memories. Cross-project: global: true.\n",
    "After compaction or big work: session_summary (Goal, Discoveries, Accomplished, Next steps).\n",
    "Plan: get_active_plan at start; create_goal/create_task for milestones.\n",
    "Secrets: list_secrets/request_secret_access; never print values."
);
