use rmcp::model::{CallToolRequestParams, ClientConfig};
use rmcp::{ClientHandler, ServiceExt};
use saispace_core::Db;
use saispace_mcp::SaiSpaceServer;
use serde_json::json;

#[derive(Default)]
struct TestClientHandler;

impl ClientHandler for TestClientHandler {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::default()
    }
}

async fn setup_test_server() -> SaiSpaceServer {
    let db = Db::connect_in_memory()
        .await
        .expect("in-memory db connection failed");
    db.migrate().await.expect("migrations failed");
    SaiSpaceServer::new(db, "test-agent".to_string())
}

async fn connect(
    server: SaiSpaceServer,
) -> (
    rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
    tokio::task::JoinHandle<()>,
) {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let server_task = tokio::spawn(async move {
        server
            .serve(server_transport)
            .await
            .expect("server start failed")
            .waiting()
            .await
            .expect("server waiting failed");
    });

    let client = TestClientHandler
        .serve(client_transport)
        .await
        .expect("client connect failed");
    (client, server_task)
}

fn text_json(result: &rmcp::model::CallToolResult) -> serde_json::Value {
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .expect("expected text content");
    serde_json::from_str(text).expect("valid json response")
}

async fn assert_expected_tools(
    client: &rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
) {
    let tools_list = client.list_tools(None).await.expect("list_tools failed");
    let tool_names: Vec<String> = tools_list
        .tools
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();

    for expected in [
        "list_projects",
        "register_project",
        "get_project_context",
        "update_project_context",
        "list_tasks",
        "create_task",
        "update_task",
        "sync_project_stack",
        "upsert_tech_rule",
        "search_memory",
        "save_memory",
        "memory_context",
        "get_memory",
        "delete_memory",
        "list_secrets",
        "request_secret_access",
    ] {
        assert!(tool_names.iter().any(|name| name == expected));
    }
}

#[tokio::test]
async fn test_tool_listing_and_calls() {
    let server = setup_test_server().await;
    let db = server.db.clone();

    let (client, server_task) = connect(server).await;

    assert_expected_tools(&client).await;

    let register_args = json!({
        "name": "Saispace Core",
        "path": "/dev/saispace",
        "context_notes": "# Initial context"
    })
    .as_object()
    .expect("json object")
    .clone();

    let register_result = client
        .call_tool(CallToolRequestParams::new("register_project").with_arguments(register_args))
        .await
        .expect("call register_project failed");

    assert_eq!(register_result.is_error, Some(false));
    let project_json = text_json(&register_result);
    let project_id = project_json["id"]
        .as_str()
        .expect("project id string")
        .to_string();

    let create_task_args = json!({
        "project_id": project_id,
        "title": "Set up MCP stdio",
        "priority": "high"
    })
    .as_object()
    .expect("json object")
    .clone();

    let create_task_result = client
        .call_tool(CallToolRequestParams::new("create_task").with_arguments(create_task_args))
        .await
        .expect("call create_task failed");

    assert_eq!(create_task_result.is_error, Some(false));

    let context_args = json!({
        "project_id": project_id
    })
    .as_object()
    .expect("json object")
    .clone();

    let context_result = client
        .call_tool(CallToolRequestParams::new("get_project_context").with_arguments(context_args))
        .await
        .expect("call get_project_context failed");

    assert_eq!(context_result.is_error, Some(false));
    let ctx_json = text_json(&context_result);
    assert_eq!(ctx_json["project"]["name"], "Saispace Core");
    assert_eq!(ctx_json["open_tasks"].as_array().expect("array").len(), 1);

    let update_notes_args = json!({
        "project_id": project_id,
        "context_notes": "Added MCP server support",
        "append": true
    })
    .as_object()
    .expect("json object")
    .clone();

    let update_notes_result = client
        .call_tool(
            CallToolRequestParams::new("update_project_context").with_arguments(update_notes_args),
        )
        .await
        .expect("call update_project_context failed");

    assert_eq!(update_notes_result.is_error, Some(false));
    let notes_json = text_json(&update_notes_result);
    let updated_notes = notes_json["context_notes"].as_str().expect("notes str");
    assert!(updated_notes.contains("# Initial context"));
    assert!(updated_notes.contains("Added MCP server support"));

    let activities = db.list_activity(50).await.expect("list_activity failed");
    assert!(activities.len() >= 4);
    assert!(activities.iter().all(|a| matches!(
        &a.actor,
        saispace_core::models::Actor::Agent { name } if name == "test-agent"
    )));

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

#[tokio::test]
async fn test_register_project_returns_existing_on_duplicate_path() {
    let server = setup_test_server().await;

    let (client, server_task) = connect(server).await;

    let register = |name: &'static str| {
        CallToolRequestParams::new("register_project").with_arguments(
            json!({ "name": name, "path": "/dev/duplicate-register" })
                .as_object()
                .expect("json object")
                .clone(),
        )
    };

    let first = client
        .call_tool(register("First"))
        .await
        .expect("first register failed");
    assert_eq!(first.is_error, Some(false));
    let first_json = text_json(&first);

    let second = client
        .call_tool(register("Second"))
        .await
        .expect("second register failed");
    assert_eq!(second.is_error, Some(false));
    let second_json = text_json(&second);

    assert_eq!(first_json["id"], second_json["id"]);
    assert_eq!(second_json["name"], "First");

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
    tool: &'static str,
    args: serde_json::Value,
) -> rmcp::model::CallToolResult {
    let arguments = args.as_object().expect("json object").clone();
    client
        .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
        .await
        .unwrap_or_else(|e| panic!("call {tool} failed: {e}"))
}

fn write_stack_fixture() -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("saispace-mcp-it-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[dependencies]\ntauri = \"2\"\nsqlx = \"0.8\"\n",
    )
    .expect("write Cargo.toml");
    std::fs::write(
        dir.join("Cargo.lock"),
        "[[package]]\nname = \"tauri\"\nversion = \"2.12.7\"\n",
    )
    .expect("write Cargo.lock");
    std::fs::write(
        dir.join("package.json"),
        r#"{"dependencies":{"react":"^19.0.0"}}"#,
    )
    .expect("write package.json");
    let path = std::fs::canonicalize(&dir)
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();
    (dir, path)
}

async fn save_stack_rules(
    client: &rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
) {
    for (technology, version, rule) in [
        (
            "tauri",
            Some("2"),
            "Permissions live in capabilities files.",
        ),
        ("tauri", Some("1"), "Use the allowlist."),
        ("django", None, "Not part of this stack."),
    ] {
        let saved = call(
            client,
            "upsert_tech_rule",
            json!({ "technology": technology, "version": version, "rule": rule }),
        )
        .await;
        assert_eq!(saved.is_error, Some(false));
    }

    let invalid = call(
        client,
        "upsert_tech_rule",
        json!({ "technology": "", "rule": "x" }),
    )
    .await;
    assert_eq!(invalid.is_error, Some(true));
}

#[tokio::test]
async fn test_stack_detection_rules_and_search_flow() {
    let (dir, path) = write_stack_fixture();

    let server = setup_test_server().await;
    let db = server.db.clone();
    let (client, server_task) = connect(server).await;

    let registered = call(
        &client,
        "register_project",
        json!({ "name": "stacked", "path": path }),
    )
    .await;
    assert_eq!(registered.is_error, Some(false));
    let registered_json = text_json(&registered);
    assert_eq!(registered_json["name"], "stacked");
    assert_eq!(registered_json["stack_error"], serde_json::Value::Null);
    let names: Vec<&str> = registered_json["stack"]
        .as_array()
        .expect("stack array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in ["tauri", "sqlx", "react"] {
        assert!(names.contains(&expected), "missing {expected} in {names:?}");
    }
    let project_id = registered_json["id"].as_str().expect("id").to_string();

    save_stack_rules(&client).await;

    let context = call(
        &client,
        "get_project_context",
        json!({ "project_id": project_id }),
    )
    .await;
    let context_json = text_json(&context);
    assert_eq!(context_json["stack"].as_array().expect("stack").len(), 3);
    let rules = context_json["tech_rules"].as_array().expect("rules");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["rule"], "Permissions live in capabilities files.");

    std::fs::write(dir.join("package.json"), r#"{"dependencies":{}}"#).expect("rewrite manifest");
    let synced = call(
        &client,
        "sync_project_stack",
        json!({ "project_id": project_id }),
    )
    .await;
    assert_eq!(synced.is_error, Some(false));
    assert!(
        text_json(&synced)
            .as_array()
            .expect("array")
            .iter()
            .all(|t| t["name"] != "react")
    );

    let found = call(
        &client,
        "search_memory",
        json!({ "query": "capabilit", "path": path }),
    )
    .await;
    assert_eq!(found.is_error, Some(false));
    let hits = text_json(&found);
    assert_eq!(hits.as_array().expect("hits").len(), 1);
    assert_eq!(hits[0]["kind"], "tech_rule");

    let bad_query = call(&client, "search_memory", json!({ "query": "  --  " })).await;
    assert_eq!(bad_query.is_error, Some(true));

    let entities: Vec<String> = db
        .list_activity(100)
        .await
        .expect("activity")
        .into_iter()
        .map(|a| format!("{:?}:{:?}", a.action, a.entity))
        .collect();
    for expected in [
        "Update:Stack",
        "Update:TechRule",
        "Read:Search",
        "Read:Project",
    ] {
        assert!(
            entities.iter().any(|e| e == expected),
            "missing activity {expected} in {entities:?}"
        );
    }

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
    let _ = std::fs::remove_dir_all(&dir);
}

async fn assert_duplicate_hint_and_context(
    client: &rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
    project_id: &str,
    first_id: &serde_json::Value,
) {
    let twin = text_json(
        &call(
            client,
            "save_memory",
            json!({ "project_id": project_id, "title": "Auth con tokens", "content": "duplicada", "kind": "decision" }),
        )
        .await,
    );
    assert_eq!(&twin["similar"][0]["id"], first_id);
    let twin_id = twin["memory"]["id"].as_str().expect("twin id").to_string();
    call(client, "delete_memory", json!({ "id": twin_id })).await;

    let bad_kind = call(
        client,
        "save_memory",
        json!({ "title": "t", "content": "c", "kind": "nope" }),
    )
    .await;
    assert_eq!(bad_kind.is_error, Some(true));

    let context = call(
        client,
        "memory_context",
        json!({ "project_id": project_id }),
    )
    .await;
    let context = text_json(&context);
    assert_eq!(context["project"]["id"], project_id);
    assert_eq!(context["preferences"][0]["title"], "Preferencia");
    let memories = context["memories"].as_array().expect("array");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["preview"], "Tokens rotativos");
}

#[tokio::test]
async fn test_memory_tools_replace_engram_flow() {
    let server = setup_test_server().await;
    let db = server.db.clone();
    let (client, server_task) = connect(server).await;

    let registered = call(
        &client,
        "register_project",
        json!({ "name": "mem-proj", "path": "/dev/mem-proj" }),
    )
    .await;
    let project_id = text_json(&registered)["id"]
        .as_str()
        .expect("id")
        .to_string();

    let first = call(
        &client,
        "save_memory",
        json!({ "project_id": project_id, "title": "Auth", "content": "Cookies firmadas", "kind": "decision", "topic_key": "arch/auth" }),
    )
    .await;
    assert_eq!(first.is_error, Some(false));
    let first_id = text_json(&first)["memory"]["id"].clone();
    assert!(first_id.is_string());

    let second = call(
        &client,
        "save_memory",
        json!({ "project_id": project_id, "title": "Auth", "content": "Tokens rotativos", "kind": "decision", "topic_key": "arch/auth" }),
    )
    .await;
    let second = text_json(&second);
    assert_eq!(second["memory"]["id"], first_id);
    assert_eq!(
        second["similar"],
        json!([]),
        "an upsert is not its own duplicate"
    );

    let global = call(
        &client,
        "save_memory",
        json!({ "title": "Preferencia", "content": "Respuestas en español", "kind": "preference", "global": true }),
    )
    .await;
    assert_eq!(
        text_json(&global)["memory"]["project_id"],
        serde_json::Value::Null
    );

    assert_duplicate_hint_and_context(&client, &project_id, &first_id).await;

    let hits = text_json(
        &call(
            &client,
            "search_memory",
            json!({ "query": "rotativ", "project_id": project_id }),
        )
        .await,
    );
    assert_eq!(hits[0]["kind"], "memory");

    let id = first_id.as_str().expect("id");
    let fetched = call(&client, "get_memory", json!({ "id": id })).await;
    assert_eq!(text_json(&fetched)["content"], "Tokens rotativos");
    assert_eq!(
        call(&client, "delete_memory", json!({ "id": id }))
            .await
            .is_error,
        Some(false)
    );
    assert_eq!(
        call(&client, "get_memory", json!({ "id": id }))
            .await
            .is_error,
        Some(true)
    );

    let activity: Vec<String> = db
        .list_activity(50)
        .await
        .expect("activity")
        .into_iter()
        .map(|a| format!("{:?}:{:?}", a.action, a.entity))
        .collect();
    assert!(activity.iter().any(|a| a == "Create:Memory"));
    assert!(activity.iter().any(|a| a == "Update:Memory"));
    assert!(activity.iter().any(|a| a == "Delete:Memory"));

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

#[tokio::test]
async fn test_secret_tools_never_expose_values() {
    let server = setup_test_server().await;
    let db = server.db.clone();
    let (client, server_task) = connect(server).await;
    let key_dir = std::env::temp_dir().join(format!("sai-mcp-vault-{}", uuid::Uuid::now_v7()));
    let vault = saispace_core::Vault::open(&key_dir.join("vault.key")).expect("vault");
    db.set_secret(&vault, "STRIPE_KEY", "sk_live_value", "pagos")
        .await
        .expect("set");

    let listed = call(&client, "list_secrets", json!({})).await;
    assert_eq!(listed.is_error, Some(false));
    let listed = text_json(&listed);
    assert_eq!(listed["secrets"][0]["name"], "STRIPE_KEY");
    assert!(listed["secrets"][0]["access"].is_null());
    assert!(!listed.to_string().contains("sk_live_value"));

    let requested = call(
        &client,
        "request_secret_access",
        json!({ "name": "STRIPE_KEY", "reason": "deploy" }),
    )
    .await;
    assert_eq!(text_json(&requested)["status"], "pending");
    let grants = db.list_secret_grants().await.expect("grants");
    assert_eq!(grants[0].agent, "test-agent");
    assert_eq!(grants[0].reason, "deploy");

    let missing = call(&client, "request_secret_access", json!({ "name": "NOPE" })).await;
    assert_eq!(missing.is_error, Some(true));

    let _ = std::fs::remove_dir_all(key_dir);
    client.cancel().await.expect("cancel");
    server_task.await.expect("server task");
}
