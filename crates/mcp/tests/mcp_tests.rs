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
    assert_eq!(context["pinned"].as_array().expect("array").len(), 0);
    let memories = context["memories"].as_array().expect("array");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["preview"], "Tokens rotativos");

    let pinned_save = call(
        client,
        "save_memory",
        json!({ "project_id": project_id, "title": "Fijada", "content": "Detalle fijado", "pinned": true }),
    )
    .await;
    assert_eq!(pinned_save.is_error, Some(false));
    assert_eq!(text_json(&pinned_save)["memory"]["pinned"], true);

    let context_pinned = call(
        client,
        "memory_context",
        json!({ "project_id": project_id }),
    )
    .await;
    let context_pinned = text_json(&context_pinned);
    let pinned_items = context_pinned["pinned"].as_array().expect("array");
    assert_eq!(pinned_items.len(), 1);
    assert_eq!(pinned_items[0]["title"], "Fijada");
    assert_eq!(pinned_items[0]["preview"], "Detalle fijado");
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

async fn register_test_project(
    client: &rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
    name: &str,
    path: &str,
) -> String {
    let result = call(
        client,
        "register_project",
        json!({ "name": name, "path": path }),
    )
    .await;
    assert_eq!(result.is_error, Some(false));
    let json = text_json(&result);
    json["id"].as_str().expect("project id string").to_string()
}

async fn create_test_task(
    client: &rmcp::service::RunningService<rmcp::RoleClient, TestClientHandler>,
    project_id: &str,
    title: &str,
    priority: Option<&str>,
) -> String {
    let mut args = json!({ "project_id": project_id, "title": title });
    if let Some(pri) = priority {
        args["priority"] = json!(pri);
    }
    let result = call(client, "create_task", args).await;
    assert_eq!(result.is_error, Some(false));
    let json = text_json(&result);
    json["id"].as_str().expect("task id string").to_string()
}

fn assert_project_summary_fields(item: &serde_json::Value, name: &str, path: &str) {
    let raw_id = item["id"].as_str().expect("project id string");
    assert!(uuid::Uuid::parse_str(raw_id).is_ok());
    assert_eq!(item["name"], name);
    assert_eq!(item["path"], path);
    assert_eq!(item["status"], "active");
    assert!(item["context_notes"].is_string());
    assert!(item["created_at"].is_string());
    assert!(item["updated_at"].is_string());
}

#[tokio::test]
async fn test_list_projects_contract() {
    let server = setup_test_server().await;
    let (client, server_task) = connect(server).await;

    let id_a = register_test_project(&client, "Project Alpha", "/dev/proj-alpha").await;
    let id_b = register_test_project(&client, "Project Beta", "/dev/proj-beta").await;

    let result = call(&client, "list_projects", json!({})).await;
    assert_eq!(result.is_error, Some(false));

    let projects = text_json(&result);
    let list = projects.as_array().expect("array of projects");
    assert!(list.len() >= 2);

    let alpha = list.iter().find(|p| p["id"] == id_a).expect("alpha found");
    assert_project_summary_fields(alpha, "Project Alpha", "/dev/proj-alpha");

    let beta = list.iter().find(|p| p["id"] == id_b).expect("beta found");
    assert_project_summary_fields(beta, "Project Beta", "/dev/proj-beta");

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

#[tokio::test]
async fn test_list_tasks_project_and_status_filtering() {
    let server = setup_test_server().await;
    let (client, server_task) = connect(server).await;

    let proj_a = register_test_project(&client, "Tasks Project A", "/dev/tasks-a").await;
    let proj_b = register_test_project(&client, "Tasks Project B", "/dev/tasks-b").await;

    let task_todo = create_test_task(&client, &proj_a, "Task Todo", Some("medium")).await;
    let task_prog = create_test_task(&client, &proj_a, "Task Prog", Some("high")).await;
    let task_done = create_test_task(&client, &proj_a, "Task Done", Some("low")).await;
    let _task_other = create_test_task(&client, &proj_b, "Task B", None).await;

    call(
        &client,
        "update_task",
        json!({ "task_id": task_prog, "status": "in_progress" }),
    )
    .await;
    call(
        &client,
        "update_task",
        json!({ "task_id": task_done, "status": "done" }),
    )
    .await;

    let all_a = text_json(&call(&client, "list_tasks", json!({ "project_id": proj_a })).await);
    let all_a_array = all_a.as_array().expect("array");
    assert_eq!(all_a_array.len(), 3);
    assert!(all_a_array.iter().all(|t| t["project_id"] == proj_a));

    let by_path = text_json(&call(&client, "list_tasks", json!({ "path": "/dev/tasks-a" })).await);
    assert_eq!(by_path.as_array().expect("array").len(), 3);

    let filtered_todo = text_json(
        &call(
            &client,
            "list_tasks",
            json!({ "project_id": proj_a, "status": "todo" }),
        )
        .await,
    );
    let todo_tasks = filtered_todo.as_array().expect("array");
    assert_eq!(todo_tasks.len(), 1);
    assert_eq!(todo_tasks[0]["id"], task_todo);

    let filtered_prog = text_json(
        &call(
            &client,
            "list_tasks",
            json!({ "project_id": proj_a, "status": "in_progress" }),
        )
        .await,
    );
    let prog_tasks = filtered_prog.as_array().expect("array");
    assert_eq!(prog_tasks.len(), 1);
    assert_eq!(prog_tasks[0]["id"], task_prog);
    assert_eq!(prog_tasks[0]["status"], "in_progress");

    let filtered_done = text_json(
        &call(
            &client,
            "list_tasks",
            json!({ "project_id": proj_a, "status": "done" }),
        )
        .await,
    );
    let done_tasks = filtered_done.as_array().expect("array");
    assert_eq!(done_tasks.len(), 1);
    assert_eq!(done_tasks[0]["id"], task_done);

    let bad_project = call(
        &client,
        "list_tasks",
        json!({ "project_id": uuid::Uuid::now_v7().to_string() }),
    )
    .await;
    assert_eq!(bad_project.is_error, Some(true));

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

#[tokio::test]
async fn test_update_task_persistence_and_activity() {
    let server = setup_test_server().await;
    let db = server.db.clone();
    let (client, server_task) = connect(server).await;

    let project_id = register_test_project(&client, "Task Update Proj", "/dev/task-update").await;
    let task_id = create_test_task(&client, &project_id, "Original Title", Some("low")).await;

    let updated = call(
        &client,
        "update_task",
        json!({
            "task_id": task_id,
            "title": "Modified Title",
            "status": "in_progress",
            "priority": "urgent"
        }),
    )
    .await;
    assert_eq!(updated.is_error, Some(false));
    let updated_json = text_json(&updated);
    assert_eq!(updated_json["title"], "Modified Title");
    assert_eq!(updated_json["status"], "in_progress");
    assert_eq!(updated_json["priority"], "urgent");

    let tasks = text_json(&call(&client, "list_tasks", json!({ "project_id": project_id })).await);
    let task = tasks
        .as_array()
        .expect("array")
        .iter()
        .find(|t| t["id"] == task_id)
        .expect("task found");
    assert_eq!(task["title"], "Modified Title");
    assert_eq!(task["status"], "in_progress");
    assert_eq!(task["priority"], "urgent");

    let unknown_uuid = uuid::Uuid::now_v7().to_string();
    let unknown_res = call(
        &client,
        "update_task",
        json!({ "task_id": unknown_uuid, "title": "Nope" }),
    )
    .await;
    assert_eq!(unknown_res.is_error, Some(true));

    let malformed_res = call(
        &client,
        "update_task",
        json!({ "task_id": "not-a-uuid", "title": "Nope" }),
    )
    .await;
    assert_eq!(malformed_res.is_error, Some(true));

    let parsed_task_id = uuid::Uuid::parse_str(&task_id).expect("uuid");
    let activities = db.list_activity(50).await.expect("list activity");
    assert!(activities.iter().any(|a| {
        a.action == saispace_core::models::ActionKind::Update
            && a.entity == saispace_core::models::EntityKind::Task
            && a.entity_id == Some(parsed_task_id)
    }));

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

#[tokio::test]
async fn test_memory_scope_all_and_orphan_isolation() {
    let server = setup_test_server().await;
    let db = server.db.clone();
    let (client, server_task) = connect(server).await;

    let proj_a = register_test_project(&client, "Scope Alpha", "/dev/scope-alpha").await;
    let proj_b = register_test_project(&client, "Scope Beta", "/dev/scope-beta").await;

    call(
        &client,
        "save_memory",
        json!({
            "project_id": proj_a,
            "title": "Alpha Secrets",
            "content": "Redis clustering architecture",
            "kind": "architecture",
            "topic_key": "arch/clustering"
        }),
    )
    .await;

    call(
        &client,
        "save_memory",
        json!({
            "project_id": proj_b,
            "title": "Beta Database",
            "content": "Postgres replication pipeline",
            "kind": "decision",
            "topic_key": "db/replication"
        }),
    )
    .await;

    let scoped_search = text_json(
        &call(
            &client,
            "search_memory",
            json!({ "query": "replication", "project_id": proj_a }),
        )
        .await,
    );
    assert_eq!(scoped_search.as_array().expect("array").len(), 0);

    let all_search = text_json(
        &call(
            &client,
            "search_memory",
            json!({ "query": "replication", "all": true }),
        )
        .await,
    );
    let all_hits = all_search.as_array().expect("array");
    assert_eq!(all_hits.len(), 1);
    assert_eq!(all_hits[0]["title"], "Beta Database");

    db.save_memory(saispace_core::models::SaveMemoryInput {
        project_id: None,
        project_name: Some("UnregisteredExternal".to_string()),
        kind: saispace_core::models::MemoryKind::Note,
        title: "Orphaned Formula".to_string(),
        content: "Proprietary quantum formula 42".to_string(),
        topic_key: Some("secret/quantum".to_string()),
    })
    .await
    .expect("save orphan memory");

    let context =
        text_json(&call(&client, "memory_context", json!({ "project_id": proj_a })).await);
    let memories = context["memories"].as_array().expect("memories array");
    assert!(memories.iter().all(|m| m["title"] != "Orphaned Formula"));
    let preferences = context["preferences"]
        .as_array()
        .expect("preferences array");
    assert!(preferences.iter().all(|p| p["title"] != "Orphaned Formula"));

    let orphan_search = text_json(
        &call(
            &client,
            "search_memory",
            json!({ "query": "quantum", "project_id": proj_a }),
        )
        .await,
    );
    assert_eq!(orphan_search.as_array().expect("array").len(), 0);

    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

#[tokio::test]
async fn test_memory_context_path_and_cwd_fallback() {
    let server = setup_test_server().await;
    let (client, server_task) = connect(server).await;

    let temp_registered =
        std::env::temp_dir().join(format!("saispace-mcp-ctx-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&temp_registered).expect("create registered dir");
    let registered_path = std::fs::canonicalize(&temp_registered)
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();

    let temp_unregistered =
        std::env::temp_dir().join(format!("saispace-mcp-unreg-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&temp_unregistered).expect("create unregistered dir");
    let unregistered_path = std::fs::canonicalize(&temp_unregistered)
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();

    let proj_id = register_test_project(&client, "Context Proj", &registered_path).await;

    call(
        &client,
        "save_memory",
        json!({
            "project_id": proj_id,
            "title": "Local Context Memory",
            "content": "Specific project details",
            "kind": "note"
        }),
    )
    .await;

    call(
        &client,
        "save_memory",
        json!({
            "title": "Global Preference",
            "content": "Always prefer concise Spanish",
            "kind": "preference",
            "global": true
        }),
    )
    .await;

    let path_context = text_json(
        &call(
            &client,
            "memory_context",
            json!({ "path": registered_path }),
        )
        .await,
    );
    assert_eq!(path_context["project"]["id"], proj_id);
    assert_eq!(path_context["memories"].as_array().expect("array").len(), 1);
    assert_eq!(
        path_context["preferences"].as_array().expect("array").len(),
        1
    );

    let unreg_context = text_json(
        &call(
            &client,
            "memory_context",
            json!({ "path": unregistered_path }),
        )
        .await,
    );
    assert!(unreg_context["project"].is_null());
    assert_eq!(
        unreg_context["memories"].as_array().expect("array").len(),
        0
    );
    assert_eq!(
        unreg_context["preferences"]
            .as_array()
            .expect("array")
            .len(),
        1
    );

    let cwd_fallback_context = text_json(&call(&client, "memory_context", json!({})).await);
    assert_eq!(
        cwd_fallback_context["preferences"]
            .as_array()
            .expect("array")
            .len(),
        1
    );

    let _ = std::fs::remove_dir_all(&temp_registered);
    let _ = std::fs::remove_dir_all(&temp_unregistered);
    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}

#[tokio::test]
async fn test_save_memory_git_autoregistration_and_root_rejection() {
    let server = setup_test_server().await;
    let db = server.db.clone();
    let (client, server_task) = connect(server).await;

    let temp_repo = std::env::temp_dir().join(format!("saispace-mcp-git-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(temp_repo.join(".git")).expect("create git repo fixture");
    let repo_path = std::fs::canonicalize(&temp_repo)
        .expect("canonicalize")
        .to_string_lossy()
        .into_owned();
    let repo_name = temp_repo
        .file_name()
        .expect("repo file name")
        .to_string_lossy()
        .into_owned();

    let orphan = db
        .save_memory(saispace_core::models::SaveMemoryInput {
            project_id: None,
            project_name: Some(repo_name.clone()),
            kind: saispace_core::models::MemoryKind::Note,
            title: "Pre-existing orphan note".to_string(),
            content: "Content saved before repo was registered".to_string(),
            topic_key: Some("notes/pre".to_string()),
        })
        .await
        .expect("save orphan");

    let saved = call(
        &client,
        "save_memory",
        json!({
            "path": repo_path,
            "title": "Git Architecture",
            "content": "Repository design details",
            "kind": "architecture"
        }),
    )
    .await;
    assert_eq!(saved.is_error, Some(false));
    let saved_json = text_json(&saved);
    let auto_project_id = saved_json["memory"]["project_id"]
        .as_str()
        .expect("project id string")
        .to_string();

    let projects = text_json(&call(&client, "list_projects", json!({})).await);
    let auto_project = projects
        .as_array()
        .expect("array")
        .iter()
        .find(|p| p["id"] == auto_project_id)
        .expect("auto-registered project in list_projects");
    assert_eq!(auto_project["name"], repo_name);
    assert_eq!(auto_project["path"], repo_path);

    let linked_orphan = db.get_memory(orphan.id).await.expect("get orphan");
    let parsed_project_id = uuid::Uuid::parse_str(&auto_project_id).expect("uuid");
    assert_eq!(linked_orphan.project_id, Some(parsed_project_id));

    let root_res = call(
        &client,
        "save_memory",
        json!({
            "path": "/",
            "title": "Root Memory",
            "content": "Root forbidden content"
        }),
    )
    .await;
    assert_eq!(root_res.is_error, Some(true));
    let err_msg = root_res
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .expect("error text");
    assert!(err_msg.contains("no project for this directory"));

    let _ = std::fs::remove_dir_all(&temp_repo);
    client.cancel().await.expect("cancel client failed");
    let _ = server_task.await;
}
