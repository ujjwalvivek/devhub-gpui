pub mod http;

use std::time::Instant;

use devhub_core::{
    append_activity, tool_git_diff, tool_git_log, tool_git_status, tool_list_projects,
    tool_list_todos, tool_list_tree, tool_project_overview, tool_read_file, tool_search_content,
    ActivityEntry, Project, ToolContext,
};
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler,
};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ProjectQuery {
    project: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct TreeQuery {
    project: String,
    max_depth: Option<usize>,
    show_hidden: Option<bool>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ReadQuery {
    project: String,
    path: String,
    start_line: Option<usize>,
    max_lines: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchQuery {
    project: String,
    query: String,
    max_hits: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct DiffQuery {
    project: String,
    path: Option<String>,
    max_chars: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct LogQuery {
    project: String,
    skip: Option<usize>,
    count: Option<usize>,
}

#[derive(Clone)]
pub struct DevHubMcp;

#[tool_router]
impl DevHubMcp {
    #[tool(
        description = "List all projects in the DevHub catalog: name, path, source (local or SSH host), type, git remote, markers, pinned state, last-modified, and open todo counts. Answered from the local catalog cache; catalog_as_of (epoch seconds) stamps its freshness."
    )]
    async fn list_projects(&self) -> Result<CallToolResult, ErrorData> {
        run("list_projects", None, move |context, _| {
            Ok(tool_list_projects(context))
        })
        .await
    }

    #[tool(
        description = "Summarize one project: README excerpt, top-level layout, live git state, last commit, and the user's todos. Live bounded reads; SSH projects cost network round-trips (seconds)."
    )]
    async fn project_overview(
        &self,
        Parameters(query): Parameters<ProjectQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        run(
            "project_overview",
            Some(query.project),
            move |context, project| {
                let project = expect_project(project)?;
                tool_project_overview(context, project)
            },
        )
        .await
    }

    #[tool(
        description = "List the file tree of a project (gitignore-aware, bounded depth and entries). Local: instant. SSH: live remote read (network round-trip)."
    )]
    async fn list_tree(
        &self,
        Parameters(query): Parameters<TreeQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let depth = query.max_depth.unwrap_or(2);
        let hidden = query.show_hidden.unwrap_or(false);
        run("list_tree", Some(query.project), move |_, project| {
            let project = expect_project(project)?;
            tool_list_tree(project, depth, hidden)
        })
        .await
    }

    #[tool(
        description = "Read a line range of a text file; the result carries the absolute path and line range for editor navigation. Binary files are refused. Local: instant. SSH: live remote read (network round-trip)."
    )]
    async fn read_file(
        &self,
        Parameters(query): Parameters<ReadQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let start = query.start_line.unwrap_or(1);
        let lines = query.max_lines.unwrap_or(400);
        run("read_file", Some(query.project), move |_, project| {
            let project = expect_project(project)?;
            tool_read_file(project, &query.path, start, lines)
        })
        .await
    }

    #[tool(
        description = "Search project file contents (gitignore-aware, bounded matches with path, line, and preview). Local: instant. SSH: live remote search (network round-trip)."
    )]
    async fn search_content(
        &self,
        Parameters(query): Parameters<SearchQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let max_hits = query.max_hits.unwrap_or(50);
        run("search_content", Some(query.project), move |_, project| {
            let project = expect_project(project)?;
            tool_search_content(project, &query.query, max_hits)
        })
        .await
    }

    #[tool(
        description = "Live git status of a project: branch, upstream, ahead/behind, and changed files with line stats. Local: instant. SSH: network round-trip."
    )]
    async fn git_status(
        &self,
        Parameters(query): Parameters<ProjectQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        run("git_status", Some(query.project), move |_, project| {
            let project = expect_project(project)?;
            tool_git_status(project)
        })
        .await
    }

    #[tool(
        description = "Unified diffs for changed files in a project, optionally filtered by path substring (bounded file count and characters). Local: instant. SSH: network round-trip."
    )]
    async fn git_diff(
        &self,
        Parameters(query): Parameters<DiffQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let max_chars = query.max_chars.unwrap_or(60_000);
        run("git_diff", Some(query.project), move |_, project| {
            let project = expect_project(project)?;
            tool_git_diff(project, query.path.as_deref(), max_chars)
        })
        .await
    }

    #[tool(
        description = "Commit history page of a project: hash, author, date, message, refs, and changed-file summary. Local: instant. SSH: network round-trip."
    )]
    async fn git_log(
        &self,
        Parameters(query): Parameters<LogQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        let skip = query.skip.unwrap_or(0);
        let count = query.count.unwrap_or(15);
        run("git_log", Some(query.project), move |_, project| {
            let project = expect_project(project)?;
            tool_git_log(project, skip, count)
        })
        .await
    }

    #[tool(
        description = "List the user's DevHub todo items for a project (pre-handoff notes kept in DevHub). Instant, local."
    )]
    async fn list_todos(
        &self,
        Parameters(query): Parameters<ProjectQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        run(
            "list_todos",
            Some(query.project),
            move |context, project| {
                let project = expect_project(project)?;
                Ok(tool_list_todos(context, project))
            },
        )
        .await
    }
}

#[tool_handler]
impl ServerHandler for DevHubMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.instructions = Some(
            "DevHub read-only project intelligence. Every tool is read-only and bounded. \
             Call list_projects to discover the catalog, then pass a project name or path \
             to the other tools. SSH-backed tools perform live remote reads and can take \
             seconds. File and search results include path and line references that editors \
             can open directly."
                .into(),
        );
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info.name = "devhub-mcp".into();
        info.server_info.title = Some("DevHub".into());
        info.server_info.version = env!("CARGO_PKG_VERSION").into();
        info
    }
}

fn expect_project(project: Option<&Project>) -> Result<&Project, String> {
    project.ok_or_else(|| "missing project".to_string())
}

fn resolve_project<'a>(context: &'a ToolContext, query: &str) -> Result<&'a Project, String> {
    context.resolve(query).ok_or_else(|| {
        let lowered = query.trim().to_lowercase();
        let suggestions = context
            .projects
            .iter()
            .filter(|project| project.name.to_lowercase().contains(&lowered))
            .take(5)
            .map(|project| project.name.clone())
            .collect::<Vec<_>>();
        if suggestions.is_empty() {
            format!("no project matches '{query}'; call list_projects to see the catalog")
        } else {
            format!(
                "no unique project matches '{query}'; did you mean: {}?",
                suggestions.join(", ")
            )
        }
    })
}

async fn run<T, F>(
    tool: &'static str,
    project_query: Option<String>,
    work: F,
) -> Result<CallToolResult, ErrorData>
where
    T: serde::Serialize + Send + 'static,
    F: FnOnce(&ToolContext, Option<&Project>) -> Result<T, String> + Send + 'static,
{
    let outcome = tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let result = (|| {
            let context = ToolContext::load()?;
            let resolved = match project_query.as_deref() {
                Some(query) => Some(resolve_project(&context, query)?),
                None => None,
            };
            work(&context, resolved)
        })();
        (started.elapsed(), result)
    })
    .await
    .map_err(|error| ErrorData::internal_error(format!("tool task failed: {error}"), None))?;

    let (duration, result) = outcome;
    let entry = ActivityEntry::new(tool, None, result.is_ok(), duration.as_millis() as u64);
    let _ = append_activity(&match &result {
        Ok(_) => entry,
        Err(error) => entry.with_detail(error.clone()),
    });

    match result {
        Ok(value) => {
            let payload = serde_json::to_string_pretty(&value).map_err(|error| {
                ErrorData::internal_error(format!("serializing tool result: {error}"), None)
            })?;
            Ok(CallToolResult::success(vec![ContentBlock::text(payload)]))
        }
        Err(error) => Err(ErrorData::invalid_params(error, None)),
    }
}
