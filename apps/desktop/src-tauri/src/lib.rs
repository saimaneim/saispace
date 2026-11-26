pub mod commands;
pub mod error;
pub mod error_log;

use commands::{
    activity::{list_activity, list_activity_by_project},
    context::{list_context_files, read_context_file, write_context_file},
    favicon::project_favicon,
    fs::list_directory,
    global::{global_ai_inventory, tech_icon},
    memory::{
        agent_memory_context, delete_memory, get_memory, link_orphan_memories, list_memories,
        list_orphan_memory_groups, memory_duplicates, memory_links, memory_stale,
        memory_usage_stats, set_memory_pinned, update_memory,
    },
    project::{
        create_project, delete_project, get_project, list_projects, reorder_projects,
        set_project_pinned, update_project,
    },
    secret::{
        decide_secret_grant, delete_secret, list_secret_grants, list_secrets, revoke_secret_grant,
        set_secret,
    },
    task::{create_task, delete_task, list_tasks, update_task},
    technology::{
        delete_tech_rule, list_project_tech_rules, list_tech_rules, list_technologies, open_url,
        search_memory, sync_project_stack, upsert_tech_rule,
    },
};
use error::CommandError;
use error_log::log_error;
use saispace_core::Db;
use std::time::Duration;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .setup(|app| {
            let db_path = saispace_core::default_db_path().map_err(CommandError::from)?;
            let app_handle = app.handle().clone();

            tauri::async_runtime::block_on(async {
                let db = Db::open(&db_path).await?;
                let vault =
                    saispace_core::Vault::open(&saispace_core::vault_key_path_for(&db_path))?;
                app.manage(vault);

                app.manage(db.clone());

                let db_watcher = db;
                let watcher_handle = app_handle;

                tauri::async_runtime::spawn(async move {
                    let mut last_id: Option<uuid::Uuid> = None;
                    let mut primed = false;
                    let mut interval = tokio::time::interval(Duration::from_millis(1500));

                    loop {
                        interval.tick().await;

                        let events = match db_watcher.list_activity(1).await {
                            Ok(events) => events,
                            Err(err) => {
                                log_error("watcher list_activity", &err);
                                continue;
                            }
                        };
                        let current_id = events.first().map(|e| e.id);
                        if current_id != last_id {
                            if primed && let Err(err) = watcher_handle.emit("saispace:activity", ())
                            {
                                log_error("watcher emit saispace:activity", &err);
                            }
                            last_id = current_id;
                            primed = true;
                        }
                    }
                });

                Ok::<(), CommandError>(())
            })?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_projects,
            project_favicon,
            get_project,
            create_project,
            update_project,
            delete_project,
            set_project_pinned,
            reorder_projects,
            list_tasks,
            create_task,
            update_task,
            delete_task,
            list_activity,
            list_activity_by_project,
            list_directory,
            list_context_files,
            read_context_file,
            write_context_file,
            list_technologies,
            sync_project_stack,
            list_tech_rules,
            list_project_tech_rules,
            upsert_tech_rule,
            delete_tech_rule,
            search_memory,
            open_url,
            list_memories,
            get_memory,
            memory_links,
            delete_memory,
            update_memory,
            set_memory_pinned,
            agent_memory_context,
            list_orphan_memory_groups,
            link_orphan_memories,
            memory_duplicates,
            memory_usage_stats,
            memory_stale,
            global_ai_inventory,
            tech_icon,
            list_secrets,
            set_secret,
            delete_secret,
            list_secret_grants,
            decide_secret_grant,
            revoke_secret_grant
        ]);

    if let Err(err) = builder.run(tauri::generate_context!()) {
        eprintln!("Failed to run tauri application: {err}");
    }
}
