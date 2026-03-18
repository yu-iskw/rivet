#![allow(clippy::needless_pass_by_value)]

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use rivet_core::{
    Analyzer, AnalyzerConfig, Language, Thresholds,
    output::{to_csv, to_json, to_sarif, to_text},
};
use rivet_runtime::{
    AnalyzerBuildContext, CollectedFiles, LanguageResolution, SkippedFile,
    analyze_files_with_cache, build_analyzer, collect_files, resolve_language,
};
use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    tool, tool_handler, tool_router, transport,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn run_stdio() -> Result<()> {
    run_stdio_with_context(AnalyzerConfig::default(), AnalyzerBuildContext::default())
}

pub fn run_stdio_with_config(config: AnalyzerConfig) -> Result<()> {
    run_stdio_with_context(config, AnalyzerBuildContext::default())
}

pub fn run_stdio_with_context(
    config: AnalyzerConfig,
    build_context: AnalyzerBuildContext,
) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?
        .block_on(async move {
            let service = RivetMcpServer::new(config, build_context)?;
            let running = service.serve(transport::stdio()).await?;
            running.waiting().await?;
            Ok(())
        })
}

#[derive(Clone)]
struct RivetMcpServer {
    analyzer: Arc<Analyzer>,
    base_config: AnalyzerConfig,
    build_context: AnalyzerBuildContext,
    tool_router: ToolRouter<Self>,
}

impl RivetMcpServer {
    fn new(config: AnalyzerConfig, build_context: AnalyzerBuildContext) -> Result<Self> {
        let analyzer = Arc::new(build_analyzer(config.clone(), &build_context)?);
        Ok(Self {
            analyzer,
            base_config: config,
            build_context,
            tool_router: Self::tool_router(),
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AnalyzeSourceParams {
    source: String,
    /// Language slug, for example `rust` or `python`.
    language: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AnalyzeFileParams {
    path: String,
    /// Optional language override. When omitted, the path extension is used.
    language: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum AnalyzeDirectoryFormat {
    Json,
    Text,
    Csv,
    Sarif,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct AnalyzeDirectoryParams {
    path: String,
    /// Optional glob pattern applied relative to the provided directory.
    glob: Option<String>,
    /// Optional rendered output format; defaults to structured JSON analysis.
    format: Option<AnalyzeDirectoryFormat>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzeDirectoryResult {
    analysis: rivet_core::ProjectAnalysis,
    skipped_files: Vec<SkippedFile>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CheckThresholdsParams {
    path: String,
    /// Optional glob pattern applied relative to the provided directory.
    glob: Option<String>,
    #[serde(rename = "max_cc")]
    max_cc: Option<u32>,
    #[serde(rename = "max_cognitive")]
    max_cognitive: Option<u32>,
    #[serde(rename = "max_length")]
    max_length: Option<u32>,
    #[serde(rename = "max_params")]
    max_params: Option<u32>,
    #[serde(rename = "max_nesting")]
    max_nesting: Option<u32>,
    #[serde(rename = "min_maintainability_index")]
    min_maintainability_index: Option<f64>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RivetMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new("rivet", env!("CARGO_PKG_VERSION"))
                .with_title("Rivet MCP")
                .with_description("AI-agent-native code complexity analysis over stdio MCP."),
        )
        .with_protocol_version(ProtocolVersion::V_2025_06_18)
        .with_instructions(
            "Use analyze_source or analyze_file for direct metrics, analyze_directory for project scans, and check_thresholds to enforce quality gates.",
        )
    }
}

#[tool_router(router = tool_router)]
impl RivetMcpServer {
    #[tool(
        name = "analyze_source",
        description = "Analyze source text and return complexity metrics."
    )]
    async fn analyze_source(
        &self,
        Parameters(params): Parameters<AnalyzeSourceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let language = parse_language(&params.language)?;
        let analysis = self
            .analyzer
            .analyze_source(params.source.as_bytes(), language, None)
            .map_err(map_analysis_error)?;
        Ok(CallToolResult::structured(analysis_to_value(&analysis)?))
    }

    #[tool(
        name = "analyze_file",
        description = "Analyze a single file path and return complexity metrics."
    )]
    async fn analyze_file(
        &self,
        Parameters(params): Parameters<AnalyzeFileParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = PathBuf::from(&params.path);
        let collected = collect_directory_files(&path, params.language.as_deref(), None).await?;
        let input = collected
            .analyzable
            .into_iter()
            .next()
            .ok_or_else(|| skipped_to_input_error(&path, &collected.skipped))?;
        let analysis = self
            .analyzer
            .analyze_source(&input.source, input.language, Some(&path))
            .map_err(map_analysis_error)?;
        Ok(CallToolResult::structured(analysis_to_value(&analysis)?))
    }

    #[tool(
        name = "analyze_directory",
        description = "Analyze all supported source files in a directory, with optional glob filtering and formatted output."
    )]
    async fn analyze_directory(
        &self,
        Parameters(params): Parameters<AnalyzeDirectoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = PathBuf::from(&params.path);
        let collected = collect_directory_files(&path, None, params.glob.as_deref()).await?;
        let project = analyze_files_with_cache(
            &self.analyzer,
            &self.base_config,
            &self.build_context,
            &collected.analyzable,
        )
        .map_err(|error| map_internal_error("failed to analyze directory", error))?;
        render_project_result(&project, collected.skipped, params.format)
    }

    #[tool(
        name = "check_thresholds",
        description = "Analyze a file or directory and evaluate threshold violations using optional override values."
    )]
    async fn check_thresholds(
        &self,
        Parameters(params): Parameters<CheckThresholdsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let path = PathBuf::from(&params.path);
        let collected = collect_directory_files(&path, None, params.glob.as_deref()).await?;
        let analyzer = build_threshold_analyzer(self, &params)?;
        let mut config = self.base_config.clone();
        apply_threshold_overrides(&mut config.thresholds, &params);
        let project = analyze_files_with_cache(
            &analyzer,
            &config,
            &self.build_context,
            &collected.analyzable,
        )
        .map_err(|error| map_internal_error("failed to analyze threshold inputs", error))?;
        let threshold_result = analyzer.check_thresholds(&project);
        structured_json_result(&threshold_result)
    }
}

fn parse_language(value: &str) -> Result<Language, ErrorData> {
    match resolve_language(value).map_err(|error| {
        ErrorData::invalid_params(format!("invalid language `{value}`: {error}"), None)
    })? {
        LanguageResolution::Full { language, .. } => Ok(language),
        LanguageResolution::ParseOnly(descriptor) => Err(ErrorData::invalid_params(
            format!("recognized but parse-only language `{}`", descriptor.id),
            None,
        )),
    }
}

fn map_input_error(error: rivet_core::RivetError) -> ErrorData {
    ErrorData::invalid_params(error.to_string(), None)
}

fn map_analysis_error(error: rivet_core::RivetError) -> ErrorData {
    match error {
        rivet_core::RivetError::UnsupportedLanguage(_) => map_input_error(error),
        other => ErrorData::internal_error(other.to_string(), None),
    }
}

fn map_internal_error(context: &str, error: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(format!("{context}: {error}"), None)
}

fn analysis_to_value<T>(analysis: &T) -> Result<Value, ErrorData>
where
    T: Serialize,
{
    serde_json::to_value(analysis)
        .map_err(|error| map_internal_error("failed to serialize analysis", error))
}

async fn collect_directory_files(
    path: &Path,
    language_override: Option<&str>,
    glob: Option<&str>,
) -> Result<CollectedFiles, ErrorData> {
    let path = path.to_path_buf();
    let language_override = language_override.map(ToOwned::to_owned);
    let glob = glob.map(ToOwned::to_owned);
    tokio::task::spawn_blocking(move || {
        collect_files(&path, language_override.as_deref(), glob.as_deref())
            .map_err(map_collection_error)
    })
    .await
    .map_err(|error| map_internal_error("failed to join directory analysis task", error))?
}

fn map_collection_error(error: anyhow::Error) -> ErrorData {
    ErrorData::invalid_params(error.to_string(), None)
}

fn skipped_to_input_error(path: &Path, skipped: &[SkippedFile]) -> ErrorData {
    let message = skipped.first().map_or_else(
        || format!("no supported source files found under {}", path.display()),
        |entry| format!("{}: {}", entry.language_id, entry.reason),
    );
    ErrorData::invalid_params(message, None)
}

fn render_project_result(
    project: &rivet_core::ProjectAnalysis,
    skipped_files: Vec<SkippedFile>,
    format: Option<AnalyzeDirectoryFormat>,
) -> Result<CallToolResult, ErrorData> {
    let response = AnalyzeDirectoryResult {
        analysis: project.clone(),
        skipped_files,
    };
    match format.unwrap_or(AnalyzeDirectoryFormat::Json) {
        AnalyzeDirectoryFormat::Json => structured_json_result(&response),
        AnalyzeDirectoryFormat::Text => {
            structured_rendered_result(&response, to_text(project), &response.skipped_files)
        }
        AnalyzeDirectoryFormat::Csv => {
            structured_rendered_result(&response, to_csv(project), &response.skipped_files)
        }
        AnalyzeDirectoryFormat::Sarif => {
            let rendered = to_sarif(project).map_err(map_analysis_error)?;
            let value = serde_json::from_str(&rendered)
                .map_err(|error| map_internal_error("failed to deserialize SARIF output", error))?;
            Ok(structured_result(value, rendered, &response.skipped_files))
        }
    }
}

fn build_threshold_analyzer(
    server: &RivetMcpServer,
    params: &CheckThresholdsParams,
) -> Result<Analyzer, ErrorData> {
    let mut config = server.base_config.clone();
    apply_threshold_overrides(&mut config.thresholds, params);
    build_analyzer(config, &server.build_context)
        .map_err(|error| map_internal_error("failed to construct threshold analyzer", error))
}

const fn apply_threshold_overrides(thresholds: &mut Thresholds, params: &CheckThresholdsParams) {
    if let Some(value) = params.max_cc {
        thresholds.max_cyclomatic_complexity = Some(value);
    }
    if let Some(value) = params.max_cognitive {
        thresholds.max_cognitive_complexity = Some(value);
    }
    if let Some(value) = params.max_length {
        thresholds.max_function_length = Some(value);
    }
    if let Some(value) = params.max_params {
        thresholds.max_parameter_count = Some(value);
    }
    if let Some(value) = params.max_nesting {
        thresholds.max_nesting_depth = Some(value);
    }
    if let Some(value) = params.min_maintainability_index {
        thresholds.min_maintainability_index = Some(value);
    }
}

fn structured_json_result<T>(value: &T) -> Result<CallToolResult, ErrorData>
where
    T: Serialize,
{
    let rendered = to_json(value).map_err(map_analysis_error)?;
    let structured = serde_json::from_str(&rendered)
        .map_err(|error| map_internal_error("failed to deserialize JSON output", error))?;
    Ok(structured_result(structured, rendered, &[]))
}

fn structured_rendered_result<T>(
    value: &T,
    rendered: String,
    skipped_files: &[SkippedFile],
) -> Result<CallToolResult, ErrorData>
where
    T: Serialize,
{
    let structured = serde_json::to_value(value)
        .map_err(|error| map_internal_error("failed to serialize structured content", error))?;
    Ok(structured_result(structured, rendered, skipped_files))
}

fn structured_result(
    value: Value,
    rendered: String,
    skipped_files: &[SkippedFile],
) -> CallToolResult {
    let mut result = CallToolResult::structured(value);
    result.content = vec![Content::text(rendered)];
    if !skipped_files.is_empty() {
        result
            .content
            .push(Content::text(render_skipped_files(skipped_files)));
    }
    result
}

fn render_skipped_files(skipped_files: &[SkippedFile]) -> String {
    let mut rendered = String::from("\nSkipped files:\n");
    for skipped in skipped_files {
        let _ = writeln!(
            rendered,
            "- {} ({}, {})",
            skipped.path.display(),
            skipped.language_id,
            skipped.reason
        );
    }
    rendered
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use rmcp::{
        RoleClient, ServiceExt,
        model::{ClientJsonRpcMessage, ServerJsonRpcMessage},
        transport::{IntoTransport, Transport},
    };
    use serde_json::{Value, json};

    use super::*;

    fn rust_source() -> &'static str {
        "fn sample(value: i32) -> i32 { if value > 0 { value } else { 0 } }"
    }

    fn multi_param_rust_source() -> &'static str {
        "fn sample(left: i32, right: i32) -> i32 { if left > right { left } else { right } }"
    }

    fn msg(raw: &str) -> ClientJsonRpcMessage {
        serde_json::from_str(raw).expect("invalid test message JSON")
    }

    fn init_request() -> ClientJsonRpcMessage {
        msg(r#"{
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "test-client", "version": "0.0.1" }
                }
            }"#)
    }

    fn initialized_notification() -> ClientJsonRpcMessage {
        msg(r#"{ "jsonrpc": "2.0", "method": "notifications/initialized" }"#)
    }

    fn start_server() -> (
        impl Transport<RoleClient>,
        tokio::task::JoinHandle<Result<()>>,
    ) {
        let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
        let handle = tokio::spawn(async move {
            let service =
                RivetMcpServer::new(AnalyzerConfig::default(), AnalyzerBuildContext::default())?;
            let running = service.serve(server_transport).await?;
            running.waiting().await?;
            Ok(())
        });

        let client = IntoTransport::<RoleClient, _, _>::into_transport(client_transport);
        (client, handle)
    }

    async fn initialize(client: &mut impl Transport<RoleClient>) -> ServerJsonRpcMessage {
        client.send(init_request()).await.expect("send initialize");
        let response = client.receive().await.expect("receive initialize");
        client
            .send(initialized_notification())
            .await
            .expect("send initialized");
        response
    }

    fn response_json(response: &ServerJsonRpcMessage) -> Value {
        serde_json::to_value(response).expect("serialize response")
    }

    #[test]
    fn check_thresholds_params_deserialize_snake_case_fields() {
        let params: CheckThresholdsParams = serde_json::from_value(json!({
            "path": "src",
            "max_cc": 0,
            "max_length": 1,
            "min_maintainability_index": 10.0
        }))
        .expect("deserialize threshold params");

        assert_eq!(params.max_cc, Some(0));
        assert_eq!(params.max_length, Some(1));
        assert_eq!(params.min_maintainability_index, Some(10.0));
    }

    fn temp_source_file(extension: &str, contents: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir().join(format!("rivet-mcp-test-{unique}.{extension}"));
        fs::write(&path, contents).expect("write temp source");
        path
    }

    fn temp_source_file_no_extension(contents: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir().join(format!("rivet-mcp-test-{unique}"));
        fs::write(&path, contents).expect("write temp source");
        path
    }

    fn temp_source_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = env::temp_dir().join(format!("rivet-mcp-dir-{unique}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[tokio::test]
    async fn initialize_reports_tool_capability() {
        let (mut client, server_handle) = start_server();
        let response = initialize(&mut client).await;
        let response = response_json(&response);

        assert_eq!(response["result"]["serverInfo"]["name"], "rivet");
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert!(response["result"]["capabilities"]["tools"].is_object());
        assert!(
            response["result"]["instructions"]
                .as_str()
                .expect("instructions")
                .contains("check_thresholds")
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
    }

    #[tokio::test]
    async fn tools_list_exposes_migrated_tools_with_generated_schemas() {
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(
                r#"{ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }"#,
            ))
            .await
            .expect("send tools/list");
        let response = client.receive().await.expect("receive tools/list");
        let response = response_json(&response);
        let tools = response["result"]["tools"].as_array().expect("tool array");
        let mut names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();

        assert_eq!(
            names,
            vec![
                "analyze_directory",
                "analyze_file",
                "analyze_source",
                "check_thresholds"
            ]
        );
        let analyze_directory = tools
            .iter()
            .find(|tool| tool["name"] == "analyze_directory")
            .expect("analyze_directory tool");
        let analyze_source = tools
            .iter()
            .find(|tool| tool["name"] == "analyze_source")
            .expect("analyze_source tool");
        let analyze_file = tools
            .iter()
            .find(|tool| tool["name"] == "analyze_file")
            .expect("analyze_file tool");
        let check_thresholds = tools
            .iter()
            .find(|tool| tool["name"] == "check_thresholds")
            .expect("check_thresholds tool");
        assert_eq!(
            analyze_directory["inputSchema"]["required"],
            json!(["path"])
        );
        assert_eq!(
            analyze_source["inputSchema"]["required"],
            json!(["source", "language"])
        );
        assert_eq!(analyze_file["inputSchema"]["required"], json!(["path"]));
        assert_eq!(check_thresholds["inputSchema"]["required"], json!(["path"]));

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
    }

    #[tokio::test]
    async fn analyze_source_returns_structured_analysis() {
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_source",
                        "arguments": {{
                            "source": "{}",
                            "language": "rust"
                        }}
                    }}
                }}"#,
                rust_source().replace('"', "\\\"")
            )))
            .await
            .expect("send tools/call");

        let response = client.receive().await.expect("receive tools/call");
        let response = response_json(&response);

        assert_eq!(response["result"]["isError"], Value::Bool(false));
        assert_eq!(response["result"]["structuredContent"]["language"], "Rust");
        assert_eq!(
            response["result"]["structuredContent"]["functions"][0]["name"],
            "sample"
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
    }

    #[tokio::test]
    async fn analyze_file_returns_structured_analysis() {
        let path = temp_source_file("rs", multi_param_rust_source());
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 4,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_file",
                        "arguments": {{
                            "path": "{}"
                        }}
                    }}
                }}"#,
                path.display()
            )))
            .await
            .expect("send tools/call");

        let response = client.receive().await.expect("receive tools/call");
        let response = response_json(&response);

        assert_eq!(response["result"]["isError"], Value::Bool(false));
        assert_eq!(response["result"]["structuredContent"]["language"], "Rust");
        assert_eq!(
            response["result"]["structuredContent"]["functions"][0]["name"],
            "sample"
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn analyze_file_rejects_parse_only_languages_explicitly() {
        let path = temp_source_file("swift", "func sample(value: Int) -> Int { value }");
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 40,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_file",
                        "arguments": {{
                            "path": "{}"
                        }}
                    }}
                }}"#,
                path.display()
            )))
            .await
            .expect("send tools/call");

        let response = client.receive().await.expect("receive tools/call");
        let response = response_json(&response);

        assert_eq!(response["error"]["code"], -32602);
        assert!(
            response["error"]["message"]
                .as_str()
                .expect("error message")
                .contains("parse-only")
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn analyze_directory_honors_glob_and_sarif_output() {
        let dir = temp_source_dir();
        let rust_path = dir.join("sample.rs");
        let ignored_path = dir.join("ignored.py");
        fs::write(&rust_path, rust_source()).expect("write rust source");
        fs::write(&ignored_path, "def ignored(value):\n    return value\n").expect("write py");

        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 7,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_directory",
                        "arguments": {{
                            "path": "{}",
                            "glob": "**/*.rs",
                            "format": "sarif"
                        }}
                    }}
                }}"#,
                dir.display()
            )))
            .await
            .expect("send tools/call");

        let response = client.receive().await.expect("receive tools/call");
        let response = response_json(&response);

        assert_eq!(response["result"]["isError"], Value::Bool(false));
        assert!(response["result"]["structuredContent"]["runs"].is_array());
        let rendered = response["result"]["content"][0]["text"]
            .as_str()
            .expect("rendered sarif");
        assert!(rendered.contains("\"runs\""));

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test]
    async fn analyze_directory_reports_parse_only_skips() {
        let dir = temp_source_dir();
        let swift_path = dir.join("sample.swift");
        let rust_path = dir.join("sample.rs");
        fs::write(&swift_path, "func sample(value: Int) -> Int { value }").expect("write swift");
        fs::write(&rust_path, rust_source()).expect("write rust source");

        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 41,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_directory",
                        "arguments": {{
                            "path": "{}",
                            "format": "json"
                        }}
                    }}
                }}"#,
                dir.display()
            )))
            .await
            .expect("send tools/call");

        let response = client.receive().await.expect("receive tools/call");
        let response = response_json(&response);

        assert_eq!(response["result"]["isError"], Value::Bool(false));
        assert_eq!(
            response["result"]["structuredContent"]["skippedFiles"][0]["language_id"],
            "swift"
        );
        assert_eq!(
            response["result"]["structuredContent"]["skippedFiles"][0]["reason"],
            "recognized but parse-only"
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test]
    async fn check_thresholds_applies_override_values() {
        let path = temp_source_file("rs", rust_source());
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 8,
                    "method": "tools/call",
                    "params": {{
                        "name": "check_thresholds",
                        "arguments": {{
                            "path": "{}",
                            "max_cc": 0
                        }}
                    }}
                }}"#,
                path.display()
            )))
            .await
            .expect("send tools/call");

        let response = client.receive().await.expect("receive tools/call");
        let response = response_json(&response);

        assert_eq!(response["result"]["isError"], Value::Bool(false));
        assert_eq!(
            response["result"]["structuredContent"]["passed"],
            Value::Bool(false)
        );
        assert_eq!(
            response["result"]["structuredContent"]["violations"][0]["metric_name"],
            "cyclomatic_complexity"
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn direct_check_thresholds_handler_applies_override_values() {
        let path = temp_source_file("rs", rust_source());
        let server =
            RivetMcpServer::new(AnalyzerConfig::default(), AnalyzerBuildContext::default())
                .expect("server");

        let result = server
            .check_thresholds(Parameters(CheckThresholdsParams {
                path: path.display().to_string(),
                glob: None,
                max_cc: Some(0),
                max_cognitive: None,
                max_length: None,
                max_params: None,
                max_nesting: None,
                min_maintainability_index: None,
            }))
            .await
            .expect("threshold tool");
        let result = serde_json::to_value(result).expect("result json");

        assert_eq!(result["structuredContent"]["passed"], Value::Bool(false));
        assert_eq!(
            result["structuredContent"]["violations"][0]["metric_name"],
            "cyclomatic_complexity"
        );

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn invalid_language_maps_to_invalid_params() {
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 5,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_source",
                        "arguments": {{
                            "source": "{}",
                            "language": "made-up"
                        }}
                    }}
                }}"#,
                rust_source().replace('"', "\\\"")
            )))
            .await
            .expect("send invalid tools/call");

        let response = client.receive().await.expect("receive invalid tools/call");
        let response = response_json(&response);

        assert!(
            response["error"]["message"]
                .as_str()
                .expect("error message")
                .contains("made-up")
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
    }

    #[tokio::test]
    async fn missing_file_maps_to_invalid_params() {
        let path = env::temp_dir().join("rivet-mcp-does-not-exist.rs");
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 9,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_file",
                        "arguments": {{
                            "path": "{}"
                        }}
                    }}
                }}"#,
                path.display()
            )))
            .await
            .expect("send invalid tools/call");

        let response = client.receive().await.expect("receive invalid tools/call");
        let response = response_json(&response);

        let message = response["error"]["message"]
            .as_str()
            .expect("error message");
        assert!(
            message.contains("rivet-mcp-does-not-exist")
                || message.contains("No such file")
                || message.contains("failed")
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
    }

    #[tokio::test]
    async fn undetectable_file_language_maps_to_invalid_params() {
        let path = temp_source_file_no_extension(rust_source());
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 10,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_file",
                        "arguments": {{
                            "path": "{}"
                        }}
                    }}
                }}"#,
                path.display()
            )))
            .await
            .expect("send invalid tools/call");

        let response = client.receive().await.expect("receive invalid tools/call");
        let response = response_json(&response);

        assert_eq!(response["error"]["code"], -32602);
        assert!(
            response["error"]["message"]
                .as_str()
                .expect("error message")
                .contains("unsupported")
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn invalid_directory_glob_maps_to_invalid_params() {
        let dir = temp_source_dir();
        fs::write(dir.join("sample.rs"), rust_source()).expect("write rust source");

        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(&format!(
                r#"{{
                    "jsonrpc": "2.0",
                    "id": 11,
                    "method": "tools/call",
                    "params": {{
                        "name": "analyze_directory",
                        "arguments": {{
                            "path": "{}",
                            "glob": "["
                        }}
                    }}
                }}"#,
                dir.display()
            )))
            .await
            .expect("send invalid tools/call");

        let response = client.receive().await.expect("receive invalid tools/call");
        let response = response_json(&response);

        assert!(
            response["error"]["message"]
                .as_str()
                .expect("error message")
                .contains("invalid glob")
        );

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
        fs::remove_dir_all(dir).expect("remove temp dir");
    }

    #[tokio::test]
    async fn missing_required_arguments_map_to_invalid_params() {
        let (mut client, server_handle) = start_server();
        let _ = initialize(&mut client).await;

        client
            .send(msg(r#"{
                    "jsonrpc": "2.0",
                    "id": 6,
                    "method": "tools/call",
                    "params": {
                        "name": "analyze_source",
                        "arguments": {
                            "source": "fn sample() {}"
                        }
                    }
                }"#))
            .await
            .expect("send invalid tools/call");

        let response = client.receive().await.expect("receive invalid tools/call");
        let response = response_json(&response);

        assert_eq!(response["error"]["code"], -32602);

        drop(client);
        server_handle
            .await
            .expect("server join")
            .expect("server stop");
    }
}
