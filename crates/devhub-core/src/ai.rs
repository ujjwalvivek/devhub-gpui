use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(not(windows))]
use keyring::Entry;
use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[cfg(windows)]
use windows::core::w;
#[cfg(windows)]
use windows::Win32::Foundation::{LocalFree, HLOCAL};
#[cfg(windows)]
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

use crate::{
    git_diff_cancellable, git_log_cancellable, git_status_summary_cancellable,
    list_project_tree_cancellable, read_project_file_cancellable,
    search_project_content_cancellable, CancellationToken, Config, GitDiffKind, Project,
    ProjectSource,
};

const CONTEXT_SCHEMA: u32 = 1;
const MAX_CONTEXT_FILES: usize = 12;
const MAX_CONTEXT_FILE_CHARS: usize = 16_000;
const MAX_CONTEXT_TOTAL_CHARS: usize = 96_000;
const MAX_QUESTION_FILES: usize = 6;
#[cfg(not(windows))]
const ZEN_SERVICE: &str = "devhub-gpui";
#[cfg(not(windows))]
const ZEN_ACCOUNT: &str = "opencode-zen-api-key";
#[cfg(windows)]
const WINDOWS_CREDENTIAL_FILE: &str = "opencode-credential.bin";
const ZEN_MODELS_URL: &str = "https://opencode.ai/zen/v1/models";
const ZEN_CHAT_URL: &str = "https://opencode.ai/zen/v1/chat/completions";
const GO_MODELS_URL: &str = "https://opencode.ai/zen/go/v1/models";
const GO_CHAT_URL: &str = "https://opencode.ai/zen/go/v1/chat/completions";
const GO_MESSAGES_URL: &str = "https://opencode.ai/zen/go/v1/messages";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenCodeService {
    Zen,
    Go,
}

impl OpenCodeService {
    pub fn label(self) -> &'static str {
        match self {
            Self::Zen => "Zen",
            Self::Go => "Go",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZenModel {
    pub id: String,
    pub free: bool,
    pub service: OpenCodeService,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZenErrorKind {
    Cancelled,
    Credential,
    Authentication,
    Network,
    RateLimited,
    Provider,
    InvalidResponse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZenError {
    pub kind: ZenErrorKind,
    pub detail: String,
}

impl ZenError {
    fn new(kind: ZenErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn status_text(&self) -> &'static str {
        match self.kind {
            ZenErrorKind::Cancelled => "Answer cancelled.",
            ZenErrorKind::Credential => "OpenCode credential unavailable.",
            ZenErrorKind::Authentication => "OpenCode API key was rejected.",
            ZenErrorKind::Network => "Network unavailable.",
            ZenErrorKind::RateLimited => "OpenCode rate limit reached.",
            ZenErrorKind::Provider => "OpenCode request failed.",
            ZenErrorKind::InvalidResponse => "OpenCode returned an invalid response.",
        }
    }
}

impl std::fmt::Display for ZenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for ZenError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectContextFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureGraph {
    pub title: String,
    pub nodes: Vec<ArchitectureNode>,
    pub edges: Vec<ArchitectureEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureResponse {
    pub narrative: String,
    pub graph: Option<ArchitectureGraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectContext {
    pub schema: u32,
    pub project_key: String,
    pub project_name: String,
    pub fingerprint: String,
    pub repository_map: Vec<String>,
    #[serde(default)]
    pub git_context: Option<String>,
    pub excerpts: Vec<ProjectContextFile>,
    pub truncated: bool,
    pub refreshed_at: u64,
}

impl ProjectContext {
    pub fn prompt_for(&self, question: &str, extra: &[ProjectContextFile]) -> String {
        let mut prompt = String::new();
        prompt.push_str("Project: ");
        prompt.push_str(&self.project_name);
        prompt.push_str("\n\nRepository map:\n");
        for path in &self.repository_map {
            prompt.push_str("- ");
            prompt.push_str(path);
            prompt.push('\n');
        }
        if self.truncated {
            prompt.push_str("- [repository map truncated]\n");
        }
        if let Some(git_context) = &self.git_context {
            prompt.push_str("\nGit context:\n");
            prompt.push_str(git_context);
            prompt.push('\n');
        }
        prompt.push_str("\nProject excerpts:\n");
        for excerpt in self.excerpts.iter().chain(extra) {
            prompt.push_str("\n--- ");
            prompt.push_str(&excerpt.path);
            prompt.push_str(" ---\n");
            prompt.push_str(&excerpt.content);
            if !excerpt.content.ends_with('\n') {
                prompt.push('\n');
            }
        }
        prompt.push_str("\nQuestion:\n");
        prompt.push_str(question.trim());
        prompt
    }
}

pub fn parse_architecture_response(
    response: &str,
    repository_map: &[String],
) -> Result<ArchitectureResponse, String> {
    let Some((start, content_start, end)) = diagram_fence(response) else {
        return Ok(ArchitectureResponse {
            narrative: response.trim().to_string(),
            graph: None,
        });
    };
    let mut graph = serde_json::from_str::<ArchitectureGraph>(&response[content_start..end])
        .map_err(|error| format!("invalid architecture graph JSON: {error}"))?;
    validate_architecture_graph(&mut graph, repository_map)?;
    let narrative = format!("{}{}", &response[..start], &response[end + 3..])
        .trim()
        .to_string();
    Ok(ArchitectureResponse {
        narrative,
        graph: Some(graph),
    })
}

fn diagram_fence(response: &str) -> Option<(usize, usize, usize)> {
    let start = response.find("```devhub-diagram")?;
    let after_marker = start + "```devhub-diagram".len();
    let content_start = response[after_marker..]
        .find('\n')
        .map(|offset| after_marker + offset + 1)
        .unwrap_or(after_marker);
    let end = response[content_start..]
        .find("```")
        .map(|offset| content_start + offset)?;
    Some((start, content_start, end))
}

fn validate_architecture_graph(
    graph: &mut ArchitectureGraph,
    repository_map: &[String],
) -> Result<(), String> {
    graph.title = graph.title.trim().chars().take(80).collect();
    if graph.title.is_empty() {
        graph.title = "Architecture".into();
    }
    if !(2..=32).contains(&graph.nodes.len()) {
        return Err("architecture graph must contain between 2 and 32 nodes".into());
    }
    if graph.edges.len() > 64 {
        return Err("architecture graph cannot contain more than 64 edges".into());
    }
    let repository_paths = repository_map
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut ids = HashSet::new();
    for node in &mut graph.nodes {
        node.id = node.id.trim().chars().take(48).collect();
        node.label = node.label.trim().chars().take(64).collect();
        node.detail = node.detail.trim().chars().take(160).collect();
        if node.id.is_empty()
            || node.label.is_empty()
            || !node
                .id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
        {
            return Err(
                "architecture node ids and labels must be non-empty and ids must be simple tokens"
                    .into(),
            );
        }
        if !ids.insert(node.id.clone()) {
            return Err(format!("duplicate architecture node id `{}`", node.id));
        }
        if let Some(path) = node.path.as_mut() {
            *path = path.trim().replace('\\', "/");
            if path.starts_with('/')
                || path.contains("../")
                || !repository_paths.contains(path.as_str())
            {
                return Err(format!(
                    "architecture node path `{path}` is not in the repository map"
                ));
            }
        }
    }
    let mut incidence = HashMap::<&str, usize>::new();
    for edge in &mut graph.edges {
        edge.from = edge.from.trim().to_string();
        edge.to = edge.to.trim().to_string();
        edge.label = edge.label.trim().chars().take(48).collect();
        if edge.from == edge.to || !ids.contains(&edge.from) || !ids.contains(&edge.to) {
            return Err("architecture edges must connect two existing, distinct nodes".into());
        }
        *incidence.entry(&edge.from).or_default() += 1;
        *incidence.entry(&edge.to).or_default() += 1;
    }
    if graph
        .nodes
        .iter()
        .any(|node| !incidence.contains_key(node.id.as_str()))
    {
        return Err("every architecture node must be connected".into());
    }
    Ok(())
}

pub fn zen_api_key_exists() -> Result<bool, ZenError> {
    Ok(read_platform_api_key()?.is_some_and(|secret| !secret.trim().is_empty()))
}

pub fn store_zen_api_key(api_key: &str) -> Result<(), ZenError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(ZenError::new(
            ZenErrorKind::Credential,
            "OpenCode API key cannot be empty",
        ));
    }
    write_platform_api_key(api_key)?;
    if read_platform_api_key()?.as_deref() == Some(api_key) {
        Ok(())
    } else {
        Err(ZenError::new(
            ZenErrorKind::Credential,
            "OS credential store did not return the saved OpenCode API key",
        ))
    }
}

pub fn delete_zen_api_key() -> Result<(), ZenError> {
    delete_platform_api_key()
}

pub fn fetch_opencode_models(cancellation: &CancellationToken) -> Result<Vec<ZenModel>, ZenError> {
    cancellation_check(cancellation)?;
    let api_key = load_zen_api_key()?;
    let client = http_client()?;
    let zen = fetch_model_catalog(
        &client,
        ZEN_MODELS_URL,
        &api_key,
        OpenCodeService::Zen,
        cancellation,
    );
    let go = fetch_model_catalog(
        &client,
        GO_MODELS_URL,
        &api_key,
        OpenCodeService::Go,
        cancellation,
    );
    let mut models = match (zen, go) {
        (Ok(mut zen), Ok(go)) => {
            zen.extend(go);
            zen
        }
        (Ok(zen), Err(_)) if !zen.is_empty() => zen,
        (Err(_), Ok(go)) if !go.is_empty() => go,
        (Err(error), _) => return Err(error),
        (_, Err(error)) => return Err(error),
    };
    models.sort_by_key(|model| {
        (
            Reverse(model.free),
            match model.service {
                OpenCodeService::Zen => 0,
                OpenCodeService::Go => 1,
            },
            model.id.to_ascii_lowercase(),
        )
    });
    Ok(models)
}

pub fn stream_opencode_answer(
    model: &ZenModel,
    prompt: &str,
    cancellation: &CancellationToken,
    mut on_delta: impl FnMut(String),
) -> Result<(), ZenError> {
    cancellation_check(cancellation)?;
    let api_key = load_zen_api_key()?;
    let system = "You are DevHub Ask Project, a read-only project guide. Answer only from the supplied repository context. Be concise and direct. Cite factual code claims with `path:line` when the excerpt contains line numbers. Say clearly when the supplied context is insufficient. Never claim to edit files, run commands, or mutate Git. When and only when the user asks for an architecture or dependency diagram, add one fenced `devhub-diagram` JSON object after the answer. Its exact shape is {\"title\":string,\"nodes\":[{\"id\":simple_token,\"label\":string,\"detail\":string,\"path\":repository_relative_path_or_null}],\"edges\":[{\"from\":node_id,\"to\":node_id,\"label\":string}]}. Use 2-20 connected nodes and only paths present in the repository map.";
    let body = json!({
        "model": model.id,
        "stream": true,
        "messages": [
            {
                "role": "system",
                "content": system
            },
            { "role": "user", "content": prompt }
        ]
    });
    let client = http_client()?;
    let chat_url = match model.service {
        OpenCodeService::Zen => ZEN_CHAT_URL,
        OpenCodeService::Go => GO_CHAT_URL,
    };
    let response = client
        .post(chat_url)
        .bearer_auth(&api_key)
        .json(&body)
        .send()
        .map_err(request_error)?;
    if response.status().is_success() {
        return read_sse(response, cancellation, &mut on_delta);
    }
    let status = response.status();
    let detail = response
        .text()
        .unwrap_or_else(|_| format!("OpenCode returned HTTP {status}"));
    if model.service != OpenCodeService::Go
        || !matches!(
            status,
            StatusCode::BAD_REQUEST
                | StatusCode::NOT_FOUND
                | StatusCode::METHOD_NOT_ALLOWED
                | StatusCode::UNPROCESSABLE_ENTITY
        )
    {
        return Err(response_error(status, detail));
    }

    cancellation_check(cancellation)?;
    let anthropic_body = json!({
        "model": model.id,
        "max_tokens": 4096,
        "stream": true,
        "system": system,
        "messages": [{ "role": "user", "content": prompt }]
    });
    let response = client
        .post(GO_MESSAGES_URL)
        .bearer_auth(&api_key)
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&anthropic_body)
        .send()
        .map_err(request_error)?;
    let response = checked_response(response)?;
    read_sse(response, cancellation, &mut on_delta)
}

fn fetch_model_catalog(
    client: &Client,
    url: &str,
    api_key: &str,
    service: OpenCodeService,
    cancellation: &CancellationToken,
) -> Result<Vec<ZenModel>, ZenError> {
    cancellation_check(cancellation)?;
    let response = client
        .get(url)
        .bearer_auth(api_key)
        .send()
        .map_err(request_error)?;
    cancellation_check(cancellation)?;
    let catalog = checked_response(response)?
        .json::<ModelCatalog>()
        .map_err(|error| ZenError::new(ZenErrorKind::InvalidResponse, error.to_string()))?;
    Ok(catalog_models(catalog, service))
}

fn catalog_models(catalog: ModelCatalog, service: OpenCodeService) -> Vec<ZenModel> {
    catalog
        .data
        .into_iter()
        .filter(|model| !model.id.trim().is_empty())
        .map(|model| ZenModel {
            free: service == OpenCodeService::Zen && model.id.ends_with("-free"),
            id: model.id,
            service,
        })
        .collect()
}

pub fn load_or_build_project_context(
    project: &Project,
    cancellation: &CancellationToken,
) -> Result<ProjectContext, String> {
    cancellation.check()?;
    let listing = list_project_tree_cancellable(project, 10, false, cancellation)?;
    let repository_map = listing
        .entries
        .iter()
        .filter(|entry| !entry.is_dir)
        .filter_map(|entry| relative_project_path(project, &entry.path))
        .filter(|path| !is_sensitive_path(path))
        .collect::<Vec<_>>();
    let git_context = build_git_context(project, cancellation);
    let fingerprint = context_fingerprint(project, &repository_map, git_context.as_deref());
    let cache_path = context_cache_path(project)?;

    if let Ok(contents) = fs::read_to_string(&cache_path) {
        if let Ok(context) = serde_json::from_str::<ProjectContext>(&contents) {
            if context.schema == CONTEXT_SCHEMA && context.fingerprint == fingerprint {
                return Ok(context);
            }
        }
    }

    let mut candidates = repository_map
        .iter()
        .filter(|path| is_text_candidate(path))
        .map(|path| (seed_rank(path), path.clone()))
        .collect::<Vec<_>>();
    candidates.sort();

    let mut excerpts = Vec::new();
    let mut total_chars = 0usize;
    for (_, relative) in candidates.into_iter().take(MAX_CONTEXT_FILES * 2) {
        cancellation.check()?;
        if excerpts.len() >= MAX_CONTEXT_FILES || total_chars >= MAX_CONTEXT_TOTAL_CHARS {
            break;
        }
        let absolute = project.path.join(Path::new(&relative));
        let Ok(content) = read_project_file_cancellable(project, &absolute, cancellation) else {
            continue;
        };
        let content = numbered_excerpt(&content, MAX_CONTEXT_FILE_CHARS);
        if content.trim().is_empty() {
            continue;
        }
        total_chars = total_chars.saturating_add(content.len());
        excerpts.push(ProjectContextFile {
            path: relative,
            content,
        });
    }

    let context = ProjectContext {
        schema: CONTEXT_SCHEMA,
        project_key: project_key(project),
        project_name: project.name.clone(),
        fingerprint,
        repository_map,
        git_context,
        excerpts,
        truncated: listing.truncated,
        refreshed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    write_context_cache(&cache_path, &context)?;
    Ok(context)
}

pub fn question_excerpts(
    project: &Project,
    context: &ProjectContext,
    question: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<ProjectContextFile>, String> {
    let terms = query_terms(question);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let seeded = context
        .excerpts
        .iter()
        .map(|excerpt| excerpt.path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut content_scores = std::collections::HashMap::<String, usize>::new();
    for term in terms.iter().take(4) {
        for hit in search_project_content_cancellable(project, term, cancellation)? {
            if let Some(relative) = relative_project_path(project, &hit.path) {
                *content_scores.entry(relative).or_default() += 5;
            }
        }
    }
    let mut candidates = context
        .repository_map
        .iter()
        .filter(|path| is_text_candidate(path) && !seeded.contains(path.as_str()))
        .filter_map(|path| {
            let score = path_score(path, &terms)
                .saturating_add(content_scores.get(path).copied().unwrap_or_default());
            (score > 0).then(|| (Reverse(score), path.clone()))
        })
        .collect::<Vec<_>>();
    candidates.sort();

    let mut excerpts = Vec::new();
    for (_, relative) in candidates.into_iter().take(MAX_QUESTION_FILES) {
        cancellation.check()?;
        let absolute = project.path.join(Path::new(&relative));
        let Ok(content) = read_project_file_cancellable(project, &absolute, cancellation) else {
            continue;
        };
        excerpts.push(ProjectContextFile {
            path: relative,
            content: numbered_excerpt(&content, MAX_CONTEXT_FILE_CHARS),
        });
    }
    Ok(excerpts)
}

pub fn clear_project_context(project: &Project) -> Result<(), String> {
    let path = context_cache_path(project)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("removing {}: {error}", path.display())),
    }
}

#[cfg(not(windows))]
fn zen_entry() -> Result<Entry, ZenError> {
    Entry::new(ZEN_SERVICE, ZEN_ACCOUNT).map_err(|error| credential_error("open", error))
}

fn load_zen_api_key() -> Result<String, ZenError> {
    read_platform_api_key()?.ok_or_else(|| {
        ZenError::new(
            ZenErrorKind::Credential,
            "No OpenCode API key was found in secure storage",
        )
    })
}

#[cfg(not(windows))]
fn credential_error(operation: &str, error: keyring::Error) -> ZenError {
    ZenError::new(
        ZenErrorKind::Credential,
        format!("cannot {operation} OpenCode API key in the OS credential store: {error}"),
    )
}

#[cfg(not(windows))]
fn read_platform_api_key() -> Result<Option<String>, ZenError> {
    match zen_entry()?.get_password() {
        Ok(secret) if secret.trim().is_empty() => Ok(None),
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(credential_error("read", error)),
    }
}

#[cfg(not(windows))]
fn write_platform_api_key(api_key: &str) -> Result<(), ZenError> {
    zen_entry()?
        .set_password(api_key)
        .map_err(|error| credential_error("store", error))
}

#[cfg(not(windows))]
fn delete_platform_api_key() -> Result<(), ZenError> {
    match zen_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(credential_error("delete", error)),
    }
}

#[cfg(windows)]
fn windows_credential_path() -> Result<PathBuf, ZenError> {
    Config::config_dir()
        .map(|directory| directory.join(WINDOWS_CREDENTIAL_FILE))
        .ok_or_else(|| {
            ZenError::new(
                ZenErrorKind::Credential,
                "cannot determine the DevHub application data directory",
            )
        })
}

#[cfg(windows)]
fn read_platform_api_key() -> Result<Option<String>, ZenError> {
    let path = windows_credential_path()?;
    let encrypted = match fs::read(&path) {
        Ok(encrypted) => encrypted,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ZenError::new(
                ZenErrorKind::Credential,
                format!("cannot read the protected OpenCode credential: {error}"),
            ));
        }
    };
    let secret = windows_unprotect(&encrypted)?;
    if secret.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(secret))
    }
}

#[cfg(windows)]
fn write_platform_api_key(api_key: &str) -> Result<(), ZenError> {
    let path = windows_credential_path()?;
    let parent = path.parent().ok_or_else(|| {
        ZenError::new(
            ZenErrorKind::Credential,
            "OpenCode credential path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        ZenError::new(
            ZenErrorKind::Credential,
            format!("cannot create the protected credential directory: {error}"),
        )
    })?;
    let encrypted = windows_protect(api_key.as_bytes())?;
    let temporary = path.with_extension("bin.tmp");
    fs::write(&temporary, encrypted).map_err(|error| {
        ZenError::new(
            ZenErrorKind::Credential,
            format!("cannot write the protected OpenCode credential: {error}"),
        )
    })?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| {
            ZenError::new(
                ZenErrorKind::Credential,
                format!("cannot replace the protected OpenCode credential: {error}"),
            )
        })?;
    }
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        ZenError::new(
            ZenErrorKind::Credential,
            format!("cannot install the protected OpenCode credential: {error}"),
        )
    })
}

#[cfg(windows)]
fn delete_platform_api_key() -> Result<(), ZenError> {
    let path = windows_credential_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ZenError::new(
            ZenErrorKind::Credential,
            format!("cannot delete the protected OpenCode credential: {error}"),
        )),
    }
}

#[cfg(windows)]
fn windows_protect(secret: &[u8]) -> Result<Vec<u8>, ZenError> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(secret.len()).map_err(|_| {
            ZenError::new(ZenErrorKind::Credential, "OpenCode API key is too large")
        })?,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            w!("DevHub OpenCode API key"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|error| {
        ZenError::new(
            ZenErrorKind::Credential,
            format!("Windows could not protect the OpenCode API key: {error}"),
        )
    })?;
    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast::<std::ffi::c_void>())));
    }
    Ok(encrypted)
}

#[cfg(windows)]
fn windows_unprotect(encrypted: &[u8]) -> Result<String, ZenError> {
    if encrypted.is_empty() {
        return Err(ZenError::new(
            ZenErrorKind::Credential,
            "the protected OpenCode credential is empty",
        ));
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(encrypted.len()).map_err(|_| {
            ZenError::new(
                ZenErrorKind::Credential,
                "the protected OpenCode credential is too large",
            )
        })?,
        pbData: encrypted.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptUnprotectData(&input, None, None, None, None, 0, &mut output) }.map_err(
        |error| {
            ZenError::new(
                ZenErrorKind::Credential,
                format!("Windows could not unlock the OpenCode API key: {error}"),
            )
        },
    )?;
    let plaintext =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast::<std::ffi::c_void>())));
    }
    String::from_utf8(plaintext).map_err(|_| {
        ZenError::new(
            ZenErrorKind::Credential,
            "the protected OpenCode credential is not valid UTF-8",
        )
    })
}

fn http_client() -> Result<Client, ZenError> {
    Client::builder()
        .user_agent(concat!("DevHub/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| ZenError::new(ZenErrorKind::Network, error.to_string()))
}

fn request_error(error: reqwest::Error) -> ZenError {
    let kind = if error.is_timeout() || error.is_connect() {
        ZenErrorKind::Network
    } else {
        ZenErrorKind::Provider
    };
    ZenError::new(kind, error.to_string())
}

fn checked_response(response: Response) -> Result<Response, ZenError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let detail = response
        .text()
        .unwrap_or_else(|_| format!("OpenCode returned HTTP {status}"));
    Err(response_error(status, detail))
}

fn response_error(status: StatusCode, detail: String) -> ZenError {
    let kind = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ZenErrorKind::Authentication,
        StatusCode::TOO_MANY_REQUESTS => ZenErrorKind::RateLimited,
        _ => ZenErrorKind::Provider,
    };
    ZenError::new(kind, detail)
}

fn read_sse(
    response: Response,
    cancellation: &CancellationToken,
    on_delta: &mut impl FnMut(String),
) -> Result<(), ZenError> {
    let mut reader = BufReader::new(response);
    let mut line = String::new();
    loop {
        cancellation_check(cancellation)?;
        line.clear();
        let read = reader.read_line(&mut line).map_err(|error| {
            ZenError::new(
                ZenErrorKind::Network,
                format!("reading Zen stream: {error}"),
            )
        })?;
        if read == 0 {
            return Ok(());
        }
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            return Ok(());
        }
        let value = serde_json::from_str::<Value>(data)
            .map_err(|error| ZenError::new(ZenErrorKind::InvalidResponse, error.to_string()))?;
        if let Some(content) = value
            .pointer("/choices/0/delta/content")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/delta/text").and_then(Value::as_str))
        {
            if !content.is_empty() {
                on_delta(content.to_string());
            }
        }
    }
}

fn cancellation_check(cancellation: &CancellationToken) -> Result<(), ZenError> {
    cancellation
        .check()
        .map_err(|_| ZenError::new(ZenErrorKind::Cancelled, "operation cancelled"))
}

#[derive(Deserialize)]
struct ModelCatalog {
    data: Vec<ModelRecord>,
}

#[derive(Deserialize)]
struct ModelRecord {
    id: String,
}

fn relative_project_path(project: &Project, path: &Path) -> Option<String> {
    path.strip_prefix(&project.path)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .filter(|relative| !relative.is_empty())
}

fn project_key(project: &Project) -> String {
    match &project.source {
        ProjectSource::Local => {
            let path = fs::canonicalize(&project.path).unwrap_or_else(|_| project.path.clone());
            format!("local:{}", path.to_string_lossy())
        }
        ProjectSource::Remote { host, .. } => {
            format!("ssh:{host}:{}", project.path.to_string_lossy())
        }
    }
}

fn context_fingerprint(
    project: &Project,
    repository_map: &[String],
    git_context: Option<&str>,
) -> String {
    let mut hasher = StableHasher::default();
    CONTEXT_SCHEMA.hash(&mut hasher);
    project_key(project).hash(&mut hasher);
    project.last_modified.hash(&mut hasher);
    git_context.hash(&mut hasher);
    for relative in repository_map {
        relative.hash(&mut hasher);
        if matches!(project.source, ProjectSource::Local) {
            let path = project.path.join(relative);
            if let Ok(metadata) = fs::metadata(path) {
                metadata.len().hash(&mut hasher);
                metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos())
                    .hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

fn context_cache_path(project: &Project) -> Result<PathBuf, String> {
    let dir = Config::cache_dir()
        .ok_or_else(|| "cannot determine the DevHub cache directory".to_string())?
        .join("ask-project");
    let mut hasher = StableHasher::default();
    project_key(project).hash(&mut hasher);
    Ok(dir.join(format!("{:016x}.json", hasher.finish())))
}

fn write_context_cache(path: &Path, context: &ProjectContext) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("creating {}: {error}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(context).map_err(|error| error.to_string())?;
    fs::write(&temporary, bytes)
        .map_err(|error| format!("writing {}: {error}", temporary.display()))?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| format!("replacing {}: {error}", path.display()))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("replacing {}: {error}", path.display()))
        .and_then(|_| prune_context_cache(parent, 8))
}

fn prune_context_cache(directory: &Path, keep: usize) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("reading {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|entry| {
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(UNIX_EPOCH);
            (modified, entry.path())
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(modified, _)| Reverse(*modified));
    for (_, path) in entries.into_iter().skip(keep) {
        fs::remove_file(&path).map_err(|error| format!("removing {}: {error}", path.display()))?;
    }
    Ok(())
}

fn build_git_context(project: &Project, cancellation: &CancellationToken) -> Option<String> {
    if !project.has_git {
        return None;
    }
    let status = git_status_summary_cancellable(project, cancellation).ok();
    let commits = git_log_cancellable(project, 12, 0, cancellation).unwrap_or_default();
    let mut output = String::new();
    if let Some(status) = &status {
        output.push_str("Branch: ");
        output.push_str(status.branch.as_deref().unwrap_or("detached"));
        if status.ahead > 0 || status.behind > 0 {
            output.push_str(&format!(
                " (ahead {}, behind {})",
                status.ahead, status.behind
            ));
        }
        output.push('\n');
        if status.changes.is_empty() {
            output.push_str("Working tree: clean\n");
        } else {
            output.push_str("Working tree:\n");
            for change in status.changes.iter().take(80) {
                output.push_str(&format!(
                    "- {} {}\n",
                    change.status_label(),
                    change.path.to_string_lossy().replace('\\', "/")
                ));
            }
        }
    }
    if let Some(status) = &status {
        let mut diff_chars = 0usize;
        for change in status.changes.iter().take(6) {
            for kind in [GitDiffKind::Unstaged, GitDiffKind::Staged] {
                let applies = match kind {
                    GitDiffKind::Unstaged => change.is_unstaged(),
                    GitDiffKind::Staged => change.is_staged(),
                };
                if !applies || diff_chars >= 24_000 {
                    continue;
                }
                let Ok(diff) = git_diff_cancellable(project, change, kind, cancellation) else {
                    continue;
                };
                let remaining = 24_000usize.saturating_sub(diff_chars);
                let excerpt = truncate_chars(&diff, remaining.min(8_000));
                if excerpt.trim().is_empty() {
                    continue;
                }
                if !output.contains("Diff excerpts:\n") {
                    output.push_str("Diff excerpts:\n");
                }
                output.push_str(&format!(
                    "--- {} ({}) ---\n{}\n",
                    change.path.to_string_lossy().replace('\\', "/"),
                    match kind {
                        GitDiffKind::Unstaged => "unstaged",
                        GitDiffKind::Staged => "staged",
                    },
                    excerpt
                ));
                diff_chars = diff_chars.saturating_add(excerpt.len());
            }
        }
    }
    if !commits.is_empty() {
        output.push_str("Recent commits:\n");
        for commit in commits {
            output.push_str(&format!(
                "- {} {} {}: {}\n",
                &commit.hash[..commit.hash.len().min(8)],
                commit.date,
                commit.author,
                commit.message.lines().next().unwrap_or_default()
            ));
        }
    }
    (!output.is_empty()).then_some(output)
}

fn seed_rank(path: &str) -> (usize, usize, String) {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let rank = match name {
        "readme.md" | "readme.mdx" | "readme" => 0,
        "cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "build.gradle"
        | "settings.gradle" | "pom.xml" | "makefile" | "cmakelists.txt" => 1,
        "adr.md" | "architecture.md" | "contributing.md" => 2,
        "lib.rs" | "main.rs" | "mod.rs" | "index.ts" | "index.tsx" | "main.ts" | "main.tsx"
        | "app.ts" | "app.tsx" => 3,
        _ if lower.starts_with("docs/") => 4,
        _ => 10,
    };
    (rank, lower.matches('/').count(), lower)
}

fn is_text_candidate(path: &str) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "rs" | "toml"
            | "md"
            | "mdx"
            | "txt"
            | "json"
            | "yaml"
            | "yml"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "hpp"
            | "cs"
            | "swift"
            | "rb"
            | "php"
            | "sh"
            | "ps1"
            | "html"
            | "css"
            | "scss"
            | "sql"
            | "xml"
    ) || Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "makefile" | "dockerfile" | "license" | "readme"
            )
        })
}

fn is_sensitive_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name,
            ".netrc"
                | ".npmrc"
                | ".pypirc"
                | "credentials"
                | "credentials.json"
                | "id_dsa"
                | "id_ecdsa"
                | "id_ed25519"
                | "id_rsa"
                | "secrets.json"
                | "secrets.toml"
                | "secrets.yaml"
                | "secrets.yml"
        )
        || matches!(
            extension,
            "jks" | "key" | "keystore" | "p12" | "pem" | "pfx"
        )
}

fn query_terms(question: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "about", "does", "from", "have", "into", "project", "that", "the", "this", "what", "when",
        "where", "which", "with", "would", "your",
    ];
    question
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .map(str::to_ascii_lowercase)
        .filter(|term| term.len() >= 3 && !STOP_WORDS.contains(&term.as_str()))
        .collect()
}

fn path_score(path: &str, terms: &[String]) -> usize {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    terms
        .iter()
        .map(|term| {
            if name.contains(term) {
                8
            } else if lower.contains(term) {
                3
            } else {
                0
            }
        })
        .sum()
}

fn numbered_excerpt(content: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, line) in content.lines().enumerate() {
        let rendered = format!("{:>5} | {line}\n", index + 1);
        if output.len().saturating_add(rendered.len()) > max_chars {
            output.push_str("[excerpt truncated]\n");
            break;
        }
        output.push_str(&rendered);
    }
    output
}

fn truncate_chars(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        return content.to_string();
    }
    let mut boundary = max_chars;
    while boundary > 0 && !content.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}\n[excerpt truncated]", &content[..boundary])
}

#[derive(Default)]
struct StableHasher(u64);

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        if self.0 == 0 {
            self.0 = 0xcbf29ce484222325;
        }
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_models_sort_before_paid_models() {
        let catalog = ModelCatalog {
            data: vec![
                ModelRecord { id: "paid".into() },
                ModelRecord {
                    id: "alpha-free".into(),
                },
            ],
        };
        let mut models = catalog
            .data
            .into_iter()
            .map(|model| ZenModel {
                free: model.id.ends_with("-free"),
                id: model.id,
                service: OpenCodeService::Zen,
            })
            .collect::<Vec<_>>();
        models.sort_by_key(|model| (Reverse(model.free), model.id.clone()));
        assert_eq!(models[0].id, "alpha-free");
    }

    #[test]
    fn catalogs_keep_all_zen_and_go_models_separate() {
        let zen = catalog_models(
            ModelCatalog {
                data: vec![
                    ModelRecord {
                        id: "zen-paid".into(),
                    },
                    ModelRecord {
                        id: "zen-free".into(),
                    },
                ],
            },
            OpenCodeService::Zen,
        );
        let go = catalog_models(
            ModelCatalog {
                data: vec![ModelRecord {
                    id: "go-model".into(),
                }],
            },
            OpenCodeService::Go,
        );

        assert_eq!(zen.len(), 2);
        assert_eq!(zen[0].id, "zen-paid");
        assert!(!zen[0].free);
        assert_eq!(zen[1].id, "zen-free");
        assert!(zen[1].free);
        assert_eq!(go.len(), 1);
        assert_eq!(go[0].service, OpenCodeService::Go);
        assert!(!go[0].free);
    }

    #[test]
    fn context_excludes_credential_shaped_paths() {
        for path in [
            ".env",
            ".env.local",
            "config/secrets.toml",
            "keys/deploy.pem",
            "home/id_ed25519",
        ] {
            assert!(is_sensitive_path(path), "{path} should be excluded");
        }
        for path in [
            "src/config.rs",
            "docs/security.md",
            "fixtures/public-key.txt",
        ] {
            assert!(!is_sensitive_path(path), "{path} should remain available");
        }
    }

    #[test]
    fn path_scoring_prefers_file_names() {
        let terms = vec!["auth".to_string()];
        assert!(path_score("src/auth.rs", &terms) > path_score("auth/src/lib.rs", &terms));
    }

    #[test]
    fn excerpts_keep_line_numbers_and_bounds() {
        let excerpt = numbered_excerpt("one\ntwo\nthree", 25);
        assert!(excerpt.starts_with("    1 | one\n"));
        assert!(excerpt.contains("[excerpt truncated]"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_dpapi_roundtrip_is_user_bound_and_not_plaintext() {
        let secret = "opencode-test-secret";
        let encrypted = windows_protect(secret.as_bytes()).unwrap();
        assert!(!encrypted
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()));
        assert_eq!(windows_unprotect(&encrypted).unwrap(), secret);
    }

    #[test]
    fn architecture_response_is_validated_against_repository_map() {
        let response = r#"The entrypoint calls the core.
```devhub-diagram
{"title":"Flow","nodes":[{"id":"app","label":"App","detail":"UI","path":"src/app.rs"},{"id":"core","label":"Core","detail":"Logic","path":"src/lib.rs"}],"edges":[{"from":"app","to":"core","label":"calls"}]}
```"#;
        let parsed = parse_architecture_response(
            response,
            &["src/app.rs".to_string(), "src/lib.rs".to_string()],
        )
        .unwrap();
        assert_eq!(parsed.narrative, "The entrypoint calls the core.");
        assert_eq!(parsed.graph.unwrap().nodes.len(), 2);
    }

    #[test]
    fn architecture_response_rejects_invented_paths() {
        let response = r#"```devhub-diagram
{"title":"Flow","nodes":[{"id":"app","label":"App","path":"missing.rs"},{"id":"core","label":"Core","path":"src/lib.rs"}],"edges":[{"from":"app","to":"core"}]}
```"#;
        assert!(parse_architecture_response(response, &["src/lib.rs".to_string()]).is_err());
    }
}
