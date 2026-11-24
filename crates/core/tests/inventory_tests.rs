use saispace_core::inventory::scan_ai_inventory;
use std::fs;

#[test]
fn inventory_reads_global_ai_config_without_env_values() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    fs::create_dir_all(h.join(".claude/skills/demo")).unwrap();
    fs::create_dir_all(h.join(".claude/plugins")).unwrap();
    fs::create_dir_all(h.join(".config/opencode")).unwrap();
    fs::create_dir_all(h.join(".agents/skills/shared")).unwrap();
    fs::write(h.join(".claude/CLAUDE.md"), "# Reglas").unwrap();
    fs::write(
        h.join(".claude/skills/demo/SKILL.md"),
        "---\nname: demo\ndescription: >\n  Line one\n  line two\n---\nbody",
    )
    .unwrap();
    fs::write(
        h.join(".agents/skills/shared/SKILL.md"),
        "---\ndescription: \"Shared\"\n---\n",
    )
    .unwrap();
    fs::write(
        h.join(".claude.json"),
        r#"{"mcpServers":{"mem":{"command":"mem-bin","args":["serve"],"env":{"TOKEN":"super-secret"}}}}"#,
    )
    .unwrap();
    fs::write(
        h.join(".claude/settings.json"),
        r#"{"enabledPlugins":{"p@m":true},"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"sai memory context"}]}]}}"#,
    )
    .unwrap();
    fs::write(
        h.join(".claude/plugins/installed_plugins.json"),
        r#"{"plugins":{"p@m":[{"version":"1.2.3"}]}}"#,
    )
    .unwrap();
    fs::write(
        h.join(".config/opencode/opencode.jsonc"),
        "{\n // comment\n \"mcp\": {\"servers\": {\"remote\": {\"type\":\"remote\",\"url\":\"https://x.dev/mcp\"}}}\n}",
    )
    .unwrap();

    let inv = scan_ai_inventory(h);
    assert_eq!(inv.instructions.len(), 1);
    assert_eq!(inv.skills.len(), 2);
    let demo = inv.skills.iter().find(|s| s.name == "demo").unwrap();
    assert_eq!(demo.description, "Line one line two");
    assert_eq!(inv.mcp_servers.len(), 2);
    let mem = inv.mcp_servers.iter().find(|s| s.name == "mem").unwrap();
    assert_eq!(mem.target, "mem-bin serve");
    assert_eq!(mem.env_keys, vec!["TOKEN".to_string()]);
    let json = serde_json::to_string(&inv).unwrap();
    assert!(!json.contains("super-secret"));
    assert_eq!(inv.plugins[0].version, "1.2.3");
    assert!(inv.plugins[0].enabled);
    assert_eq!(inv.hooks[0].command, "sai memory context");
    assert_eq!(inv.warnings.len(), 0);
}
