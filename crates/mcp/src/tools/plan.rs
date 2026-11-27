//! Plan tools: a two-level plan (goals → tasks) with compact JSON, so an
//! agent gets the state of the work and the memory context in one call.

use crate::server::SaiSpaceServer;
use crate::tools::task::task_update_view;
use crate::tools::{format_error, format_success};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use saispace_core::memory_render::excerpt;
use saispace_core::models::{
    ActionKind, EntityKind, GoalStatus, MemoryScope, Plan, PlanCounts, PlanFilter,
    RecordActivityInput, TaskStatus, UpdateGoalInput, UpdateTaskInput,
};
use saispace_core::{CoreError, short_id};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ActivePlanParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateGoalParams {
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub title: String,
    /// Short "done when" description.
    pub criteria: Option<String>,
    /// Initial task breakdown, created with the goal in one transaction.
    #[serde(default)]
    pub tasks: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateGoalParams {
    /// Full id or the 8-digit short id shown in the plan.
    pub id: String,
    pub status: Option<GoalStatusParam>,
    pub title: Option<String>,
    pub criteria: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskIdParams {
    /// Full id or the 8-digit short id shown in the plan.
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatusParam {
    Active,
    Completed,
    Archived,
}

impl From<GoalStatusParam> for GoalStatus {
    fn from(p: GoalStatusParam) -> Self {
        match p {
            GoalStatusParam::Active => GoalStatus::Active,
            GoalStatusParam::Completed => GoalStatus::Completed,
            GoalStatusParam::Archived => GoalStatus::Archived,
        }
    }
}

#[derive(Debug, Serialize)]
struct PlanProjectView {
    id: Uuid,
    name: String,
}

#[derive(Debug, Serialize)]
struct PlanTaskView {
    id: String,
    title: String,
    status: TaskStatus,
}

impl PlanTaskView {
    fn new(task: &saispace_core::models::Task) -> Self {
        Self {
            id: short_id(task.id),
            title: task.title.clone(),
            status: task.status,
        }
    }
}

#[derive(Debug, Serialize)]
struct PlanGoalView {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    criteria: Option<String>,
    done: i64,
    total: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tasks: Vec<PlanTaskView>,
}

#[derive(Debug, Serialize)]
struct GoalView {
    id: String,
    title: String,
    status: GoalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    criteria: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tasks: Vec<PlanTaskView>,
}

#[derive(Debug, Serialize)]
struct SessionExcerptView {
    id: String,
    excerpt: String,
}

#[derive(Debug, Serialize)]
struct RecentMemoryView {
    id: String,
    title: String,
}

#[derive(Debug, Serialize)]
struct PlanMemoryView {
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<SessionExcerptView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    recent: Vec<RecentMemoryView>,
}

impl PlanMemoryView {
    const fn is_empty(&self) -> bool {
        self.session.is_none() && self.recent.is_empty()
    }
}

#[derive(Debug, Serialize)]
struct ActivePlanView {
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<PlanProjectView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    goals: Vec<PlanGoalView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    loose_tasks: Vec<PlanTaskView>,
    #[serde(skip_serializing_if = "PlanMemoryView::is_empty")]
    memory: PlanMemoryView,
}

#[tool_router(router = plan_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "get_active_plan",
        description = "Read the active plan of the current project (project_id, path, or working directory) in one call: active goals with their progress and pending tasks, open tasks without a goal, and the memory context (latest session summary and recent memories). Call it when starting work in a project"
    )]
    async fn get_active_plan(
        &self,
        Parameters(params): Parameters<ActivePlanParams>,
    ) -> CallToolResult {
        let project = match self
            .memory_project(params.project_id.as_deref(), params.path.as_deref(), false)
            .await
        {
            Ok(project) => project,
            Err(result) => return *result,
        };
        let plan = match &project {
            Some(p) => match self.db.get_plan(p.id, PlanFilter::Active).await {
                Ok(plan) => plan,
                Err(err) => return format_error(&err),
            },
            None => Plan {
                goals: Vec::new(),
                loose_tasks: Vec::new(),
                counts: PlanCounts {
                    active: 0,
                    completed: 0,
                    all: 0,
                },
            },
        };
        let scope = MemoryScope::for_project(project.as_ref().map(|p| p.id));
        let context = match self.db.memory_context(scope, 5).await {
            Ok(context) => context,
            Err(err) => return format_error(&err),
        };
        if let Some(p) = &project {
            let _ = self
                .db
                .record_activity(RecordActivityInput {
                    project_id: Some(p.id),
                    actor: self.actor(),
                    action: ActionKind::Read,
                    entity: EntityKind::Goal,
                    entity_id: None,
                    details: None,
                })
                .await;
        }
        let is_pending = |task: &saispace_core::models::Task| {
            task.status != TaskStatus::Done && task.status != TaskStatus::Cancelled
        };
        format_success(&ActivePlanView {
            project: project.map(|p| PlanProjectView {
                id: p.id,
                name: p.name,
            }),
            goals: plan
                .goals
                .iter()
                .map(|g| PlanGoalView {
                    id: short_id(g.goal.id),
                    title: g.goal.title.clone(),
                    criteria: g.goal.criteria.clone(),
                    done: g.done,
                    total: g.total,
                    tasks: g
                        .tasks
                        .iter()
                        .filter(|task| is_pending(task))
                        .map(PlanTaskView::new)
                        .collect(),
                })
                .collect(),
            loose_tasks: plan.loose_tasks.iter().map(PlanTaskView::new).collect(),
            memory: PlanMemoryView {
                session: context.last_session.map(|m| SessionExcerptView {
                    id: short_id(m.id),
                    excerpt: excerpt(&m.content, 300),
                }),
                recent: context
                    .memories
                    .into_iter()
                    .take(5)
                    .map(|m| RecentMemoryView {
                        id: short_id(m.id),
                        title: m.title,
                    })
                    .collect(),
            },
        })
    }

    #[tool(
        name = "create_goal",
        description = "Create a milestone of the current project with its task breakdown in one call. Optional criteria is the short \"done when\". Record only milestones and steps that matter across sessions, not ephemeral micro-steps"
    )]
    async fn create_goal(
        &self,
        Parameters(params): Parameters<CreateGoalParams>,
    ) -> CallToolResult {
        let project = match self
            .memory_project(params.project_id.as_deref(), params.path.as_deref(), true)
            .await
        {
            Ok(Some(project)) => project,
            Ok(None) => {
                return format_error(&CoreError::Validation(
                    "no project for this directory: pass the project path".to_string(),
                ));
            }
            Err(result) => return *result,
        };
        let created = match self
            .db
            .create_goal_with_tasks(project.id, params.title, params.criteria, &params.tasks)
            .await
        {
            Ok(created) => created,
            Err(err) => return format_error(&err),
        };
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: Some(project.id),
                actor: self.actor(),
                action: ActionKind::Create,
                entity: EntityKind::Goal,
                entity_id: Some(created.goal.id),
                details: None,
            })
            .await;
        format_success(&GoalView {
            id: short_id(created.goal.id),
            title: created.goal.title,
            status: created.goal.status,
            criteria: created.goal.criteria,
            tasks: created.tasks.iter().map(PlanTaskView::new).collect(),
        })
    }

    #[tool(
        name = "update_goal",
        description = "Update a goal (full or short id): complete it once every task is done, reactivate it, archive it, rename it or adjust its criteria"
    )]
    async fn update_goal(
        &self,
        Parameters(params): Parameters<UpdateGoalParams>,
    ) -> CallToolResult {
        let goal_id = match self.db.resolve_goal_id(&params.id).await {
            Ok(id) => id,
            Err(err) => return format_error(&err),
        };
        let input = UpdateGoalInput {
            title: params.title,
            criteria: params.criteria,
            status: params.status.map(Into::into),
            position: None,
        };
        let completed = input.status == Some(GoalStatus::Completed);
        let goal = match self.db.update_goal(goal_id, input).await {
            Ok(goal) => goal,
            Err(err) => return format_error(&err),
        };
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: Some(goal.project_id),
                actor: self.actor(),
                action: ActionKind::Update,
                entity: EntityKind::Goal,
                entity_id: Some(goal.id),
                details: completed.then(|| serde_json::json!({ "status": "completed" })),
            })
            .await;
        format_success(&GoalView {
            id: short_id(goal.id),
            title: goal.title,
            status: goal.status,
            criteria: goal.criteria,
            tasks: Vec::new(),
        })
    }

    #[tool(
        name = "complete_task",
        description = "Mark a task as done (full or short id) and report its goal's new progress; complete the goal with update_goal once every task is done"
    )]
    async fn complete_task(&self, Parameters(params): Parameters<TaskIdParams>) -> CallToolResult {
        let task_id = match self.db.resolve_task_id(&params.id).await {
            Ok(id) => id,
            Err(err) => return format_error(&err),
        };
        let updated = match self
            .db
            .update_task(
                task_id,
                UpdateTaskInput {
                    status: Some(TaskStatus::Done),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(updated) => updated,
            Err(err) => return format_error(&err),
        };
        let _ = self
            .db
            .record_activity(RecordActivityInput {
                project_id: Some(updated.task.project_id),
                actor: self.actor(),
                action: ActionKind::Update,
                entity: EntityKind::Task,
                entity_id: Some(updated.task.id),
                details: Some(serde_json::json!({ "status": "done" })),
            })
            .await;
        format_success(&task_update_view(&updated))
    }
}
