use crate::server::SaiSpaceServer;
use crate::tools::{format_error, format_success};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};
use saispace_core::models::GrantStatus;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EmptyParams {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RequestSecretParams {
    pub name: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct SecretRow {
    name: String,
    description: String,
    access: Option<GrantStatus>,
}

const USAGE: &str = "Run commands with: sai secret run -s NAME[=ENV_VAR] -- <command>. The value is injected as an env var and masked in output; never print or store it.";

#[tool_router(router = secret_tool_router, vis = "pub")]
impl SaiSpaceServer {
    #[tool(
        name = "list_secrets",
        description = "Stored secret names (never values) and this agent's access. Use approved ones with: sai secret run -s NAME[=ENV_VAR] -- <command>"
    )]
    async fn list_secrets(&self, Parameters(_): Parameters<EmptyParams>) -> CallToolResult {
        let secrets = match self.db.list_secrets().await {
            Ok(items) => items,
            Err(err) => return format_error(&err),
        };
        let grants = match self.db.list_secret_grants().await {
            Ok(items) => items,
            Err(err) => return format_error(&err),
        };
        let rows: Vec<SecretRow> = secrets
            .into_iter()
            .map(|s| SecretRow {
                access: grants
                    .iter()
                    .find(|g| g.secret_name == s.name && g.agent == self.agent_name)
                    .map(|g| g.status),
                name: s.name,
                description: s.description,
            })
            .collect();
        format_success(
            &serde_json::json!({ "agent": self.agent_name, "secrets": rows, "usage": USAGE }),
        )
    }

    #[tool(
        name = "request_secret_access",
        description = "Ask the user to approve a stored secret (once, in the app); then use it with sai secret run without seeing its value"
    )]
    async fn request_secret_access(
        &self,
        Parameters(params): Parameters<RequestSecretParams>,
    ) -> CallToolResult {
        match self
            .db
            .request_secret_access(
                &params.name,
                &self.agent_name,
                params.reason.as_deref().unwrap_or(""),
            )
            .await
        {
            Ok(grant) => format_success(&serde_json::json!({
                "secret": grant.secret_name,
                "agent": grant.agent,
                "status": grant.status,
                "usage": USAGE,
            })),
            Err(err) => format_error(&err),
        }
    }
}
