use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use devhub_core::{
    save_project_todos, save_projects, todo_key, Config, Project, ProjectSource, ProjectType,
    TodoItem,
};
use devhub_mcp::http::McpHttpServer;
use rmcp::{
    model::{CallToolRequestParams, ClientInfo},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    ServiceExt,
};

const STATE_DIR_ENV: &str = "DEVHUB_GPUI_STATE_DIR";
const TOOL_NAMES: [&str; 9] = [
    "git_diff",
    "git_log",
    "git_status",
    "list_projects",
    "list_todos",
    "list_tree",
    "project_overview",
    "read_file",
    "search_content",
];

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "devhub-mcp-http-client-{}-{nonce}",
            std::process::id()
        ));
        let project_root = root.join("fixture");
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        std::fs::write(project_root.join("README.md"), "# Fixture\n").unwrap();
        std::fs::write(
            project_root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            project_root.join("src/main.rs"),
            "fn main() { /* alpha */ }\n",
        )
        .unwrap();

        run_git(&project_root, &["init", "-q"]);
        run_git(&project_root, &["config", "user.name", "DevHub Test"]);
        run_git(
            &project_root,
            &["config", "user.email", "devhub@example.invalid"],
        );
        run_git(&project_root, &["add", "."]);
        run_git(&project_root, &["commit", "-qm", "initial"]);
        std::fs::write(
            project_root.join("src/main.rs"),
            "fn main() { println!(\"alpha\"); }\n",
        )
        .unwrap();

        std::env::set_var(STATE_DIR_ENV, root.join("state"));
        Config::default().save().unwrap();
        let project = Project {
            name: "fixture".to_string(),
            path: project_root,
            source: ProjectSource::Local,
            project_type: ProjectType::Rust,
            has_git: true,
            git_remote: None,
            markers_found: vec!["Cargo.toml".to_string()],
            last_modified: None,
            search_key: "fixture".to_string(),
        };
        save_projects(std::slice::from_ref(&project)).unwrap();
        save_project_todos(
            &todo_key(&project),
            &[TodoItem::new("verify MCP transport")],
        )
        .unwrap();

        Self { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::env::remove_var(STATE_DIR_ENV);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run_git(directory: &std::path::Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .status()
        .expect("run git");
    assert!(status.success(), "git {arguments:?} failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_client_calls_all_tools_repeatedly_and_reconnects() {
    let _fixture = Fixture::new();
    let server = McpHttpServer::start(0, "secret".to_string()).expect("start server");
    let uri = format!("http://{}/mcp", server.address());
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(uri.clone()).auth_header("secret"),
    );
    let client = ClientInfo::default()
        .serve(transport)
        .await
        .expect("initialize real MCP client");

    let mut names = client
        .list_all_tools()
        .await
        .expect("list tools")
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, TOOL_NAMES);

    let calls = [
        (
            "project_overview",
            serde_json::json!({"project": "fixture"}),
        ),
        (
            "list_tree",
            serde_json::json!({"project": "fixture", "max_depth": 2}),
        ),
        (
            "read_file",
            serde_json::json!({"project": "fixture", "path": "README.md"}),
        ),
        (
            "search_content",
            serde_json::json!({"project": "fixture", "query": "alpha"}),
        ),
        ("git_status", serde_json::json!({"project": "fixture"})),
        ("git_diff", serde_json::json!({"project": "fixture"})),
        ("git_log", serde_json::json!({"project": "fixture"})),
        ("list_todos", serde_json::json!({"project": "fixture"})),
    ];

    client
        .call_tool(CallToolRequestParams::new("list_projects"))
        .await
        .expect("call list_projects");
    for (name, value) in calls {
        let arguments = serde_json::from_value(value).expect("object arguments");
        let result = client
            .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
            .await
            .unwrap_or_else(|error| panic!("{name} failed: {error}"));
        assert_ne!(result.is_error, Some(true), "{name} returned a tool error");

        client
            .call_tool(CallToolRequestParams::new("list_projects"))
            .await
            .unwrap_or_else(|error| panic!("transport closed after {name}: {error}"));
    }

    client.cancel().await.expect("close first MCP client");

    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(uri).auth_header("secret"),
    );
    let second_client = ClientInfo::default()
        .serve(transport)
        .await
        .expect("reconnect a fresh MCP client");
    second_client
        .call_tool(CallToolRequestParams::new("list_projects"))
        .await
        .expect("call tool after reconnect");
    second_client
        .cancel()
        .await
        .expect("close second MCP client");

    server.stop().expect("stop server cleanly");
}
