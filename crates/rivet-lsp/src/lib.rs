use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};

use anyhow::Result;
use rivet_core::{Analyzer, AnalyzerConfig, FileAnalysis, Language, ThresholdViolation};
use rivet_runtime::{AnalyzerBuildContext, analyze_source_with_cache, build_analyzer};
use serde_json::Value;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tower_lsp_server::{
    Client, LanguageServer, LspService, Server,
    jsonrpc::Result as LspResult,
    ls_types::{
        CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
        CodeActionProviderCapability, CodeActionResponse, CodeLens, CodeLensOptions,
        CodeLensParams, Command, Diagnostic, DiagnosticSeverity as LspDiagnosticSeverity,
        DidChangeConfigurationParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentSymbol, DocumentSymbolParams,
        DocumentSymbolResponse, Hover, HoverContents, HoverParams, HoverProviderCapability,
        InitializeParams, InitializeResult, InitializedParams, InlayHint, InlayHintKind,
        InlayHintLabel, InlayHintParams, MarkupContent, MarkupKind, MessageType, NumberOrString,
        OneOf, Position, Range, SaveOptions, ServerCapabilities, SymbolKind,
        TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
        TextDocumentSyncSaveOptions, TextEdit, Uri, WorkspaceEdit,
    },
};

pub mod state;

use state::{DocumentEntry, DocumentState};

#[derive(Debug, Clone, Copy)]
pub enum DiagnosticSeverity {
    Warning,
    Information,
}

impl DiagnosticSeverity {
    const fn as_lsp(self) -> LspDiagnosticSeverity {
        match self {
            Self::Warning => LspDiagnosticSeverity::WARNING,
            Self::Information => LspDiagnosticSeverity::INFORMATION,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LspConfig {
    pub analyzer_config: AnalyzerConfig,
    pub analyze_on_change: bool,
    pub debounce_ms: u64,
    pub enable_code_lenses: bool,
    pub enable_hover: bool,
    pub diagnostic_severity: DiagnosticSeverity,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            analyzer_config: AnalyzerConfig::default(),
            analyze_on_change: true,
            debounce_ms: 300,
            enable_code_lenses: true,
            enable_hover: true,
            diagnostic_severity: DiagnosticSeverity::Warning,
        }
    }
}

struct BackendState {
    analyzer: RwLock<Analyzer>,
    config: RwLock<LspConfig>,
    build_context: AnalyzerBuildContext,
    documents: DocumentState,
}

impl BackendState {
    fn new(analyzer: Analyzer, config: LspConfig, build_context: AnalyzerBuildContext) -> Self {
        Self {
            analyzer: RwLock::new(analyzer),
            config: RwLock::new(config),
            build_context,
            documents: DocumentState::new(),
        }
    }
}

#[derive(Clone)]
struct RivetLanguageServer {
    client: Client,
    state: Arc<BackendState>,
}

impl RivetLanguageServer {
    const fn new(client: Client, state: Arc<BackendState>) -> Self {
        Self { client, state }
    }

    async fn log_warning(&self, message: impl Into<String>) {
        self.client
            .log_message(MessageType::WARNING, message.into())
            .await;
    }

    fn schedule_analysis(&self, uri: Uri, revision: u64, debounce: Duration) {
        let this = self.clone();
        tokio::spawn(async move {
            if !debounce.is_zero() {
                sleep(debounce).await;
            }
            this.run_analysis(uri, revision).await;
        });
    }

    async fn run_analysis(&self, uri: Uri, revision: u64) {
        let uri_key = uri.to_string();
        let Some(entry) = self.state.documents.get(&uri_key) else {
            return;
        };
        if entry.revision != revision {
            return;
        }

        let analysis_result = {
            let analyzer_config = self.state.config.read().await.analyzer_config.clone();
            let analyzer = self.state.analyzer.read().await;
            if entry.dirty {
                analyzer.analyze_source(entry.source.as_bytes(), entry.language, None)
            } else {
                let file_path = uri.to_file_path();
                analyze_source_with_cache(
                    &analyzer,
                    &analyzer_config,
                    &self.state.build_context,
                    entry.source.as_bytes(),
                    entry.language,
                    file_path.as_deref(),
                )
                .map_err(|error| rivet_core::RivetError::Analysis(error.to_string()))
            }
        };

        match analysis_result {
            Ok(analysis) => {
                let Some(updated_entry) = self.state.documents.set_analysis_if_revision(
                    &uri_key,
                    revision,
                    Some(analysis.clone()),
                ) else {
                    return;
                };
                self.publish_diagnostics(uri, &updated_entry, &analysis)
                    .await;
            }
            Err(error) => {
                if self
                    .state
                    .documents
                    .set_analysis_if_revision(&uri_key, revision, None)
                    .is_none()
                {
                    return;
                }
                self.log_warning(format!("failed to analyze {}: {error}", uri.as_str()))
                    .await;
                if let Some(current_entry) = self.state.documents.get(&uri_key) {
                    self.client
                        .publish_diagnostics(uri, Vec::new(), Some(current_entry.version))
                        .await;
                }
            }
        }
    }

    async fn publish_diagnostics(&self, uri: Uri, entry: &DocumentEntry, analysis: &FileAnalysis) {
        let severity = self.state.config.read().await.diagnostic_severity;
        let violations = {
            let analyzer = self.state.analyzer.read().await;
            analyzer
                .check_file_thresholds(analysis)
                .into_iter()
                .filter(|violation| !is_suppressed(entry, violation))
                .collect::<Vec<_>>()
        };
        let diagnostics = violations
            .iter()
            .map(|violation| diagnostic(violation, severity))
            .collect::<Vec<_>>();

        self.client
            .publish_diagnostics(uri, diagnostics, Some(entry.version))
            .await;
    }

    async fn apply_configuration_value(&self, settings: &Value) {
        let mut config = self.state.config.read().await.clone();
        let rebuild_analyzer = update_config_from_settings(settings, &mut config);

        if rebuild_analyzer && self.rebuild_analyzer(&config).await.is_err() {
            return;
        }

        *self.state.config.write().await = config;
        self.reanalyze_open_documents().await;
    }

    async fn rebuild_analyzer(&self, config: &LspConfig) -> Result<(), ()> {
        match build_analyzer(config.analyzer_config.clone(), &self.state.build_context) {
            Ok(analyzer) => {
                *self.state.analyzer.write().await = analyzer;
                Ok(())
            }
            Err(error) => {
                self.log_warning(format!(
                    "failed to rebuild analyzer from LSP settings: {error}"
                ))
                .await;
                Err(())
            }
        }
    }

    async fn reanalyze_open_documents(&self) {
        for uri in self.state.documents.uris() {
            match Uri::from_str(&uri) {
                Ok(uri) => {
                    if let Some(entry) = self.state.documents.bump_revision(&uri.to_string()) {
                        self.schedule_analysis(uri, entry.revision, Duration::ZERO);
                    }
                }
                Err(error) => {
                    self.log_warning(format!("failed to parse document URI `{uri}`: {error}"))
                        .await;
                }
            }
        }
    }

    async fn apply_initialize_options(&self, params: &InitializeParams) {
        if let Some(options) = params.initialization_options.as_ref() {
            self.apply_configuration_value(options).await;
        }
    }

    fn cached_analysis(&self, uri: &Uri) -> Option<DocumentEntry> {
        self.state.documents.get(&uri.to_string())
    }
}

impl LanguageServer for RivetLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        self.apply_initialize_options(&params).await;
        let config = self.state.config.read().await.clone();
        Ok(InitializeResult {
            capabilities: initialize_capabilities(&config),
            server_info: None,
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.apply_configuration_value(&params.settings).await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        let uri = document.uri;
        let uri_key = uri.to_string();

        let Ok(language) = Language::from_str(&document.language_id) else {
            self.log_warning(format!(
                "unsupported language id `{}` for {}",
                document.language_id,
                uri.as_str()
            ))
            .await;
            self.client
                .publish_diagnostics(uri, Vec::new(), Some(document.version))
                .await;
            return;
        };

        self.state
            .documents
            .open(&uri_key, document.text, document.version, language);
        if let Some(entry) = self.state.documents.get(&uri_key) {
            self.schedule_analysis(uri, entry.revision, Duration::ZERO);
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let uri_key = uri.to_string();
        let Some(change) = params.content_changes.last() else {
            return;
        };

        let Some(entry) = self.state.documents.update_source(
            &uri_key,
            change.text.clone(),
            params.text_document.version,
        ) else {
            return;
        };

        if self.state.config.read().await.analyze_on_change {
            let debounce_ms = self.state.config.read().await.debounce_ms;
            self.schedule_analysis(uri, entry.revision, Duration::from_millis(debounce_ms));
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let uri_key = uri.to_string();

        if let Some(text) = params.text {
            let _ = self.state.documents.replace_saved_text(&uri_key, text);
        }
        let entry = self
            .state
            .documents
            .bump_revision(&uri_key)
            .or_else(|| self.state.documents.get(&uri_key));
        if let Some(entry) = entry {
            self.schedule_analysis(uri, entry.revision, Duration::ZERO);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let _ = self.state.documents.remove(&uri.to_string());
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        if !self.state.config.read().await.enable_hover {
            return Ok(None);
        }
        let uri = params.text_document_position_params.text_document.uri;
        let line = params.text_document_position_params.position.line;
        let Some(entry) = self.cached_analysis(&uri) else {
            return Ok(None);
        };
        let Some(analysis) = entry.analysis.as_ref() else {
            return Ok(None);
        };
        let Some(function) = analysis.functions.iter().find(|function| {
            let start = function.start_line.saturating_sub(1);
            let end = function.end_line.saturating_sub(1);
            line >= start && line <= end
        }) else {
            return Ok(None);
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "### `{}`\n\nCC: **{}**\n\nCognitive: **{}**\n\nParams: **{}**\n\nNLOC: **{}**\n\nNesting: **{}**\n\nMaintainability (file): **{:.2}**",
                    function.name,
                    function.cyclomatic_complexity,
                    function.cognitive_complexity,
                    function.parameter_count,
                    function.nloc,
                    function.nesting_depth,
                    analysis.file_metrics.maintainability_index
                ),
            }),
            range: Some(function_range(
                function.start_line,
                function.start_column,
                function.end_line,
                function.end_column,
            )),
        }))
    }

    async fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        if !self.state.config.read().await.enable_code_lenses {
            return Ok(Some(Vec::new()));
        }
        let uri = params.text_document.uri;
        let Some(entry) = self.cached_analysis(&uri) else {
            return Ok(Some(Vec::new()));
        };
        let Some(analysis) = entry.analysis.as_ref() else {
            return Ok(Some(Vec::new()));
        };

        Ok(Some(
            analysis
                .functions
                .iter()
                .map(|function| CodeLens {
                    range: Range {
                        start: Position {
                            line: function.start_line.saturating_sub(1),
                            character: 0,
                        },
                        end: Position {
                            line: function.start_line.saturating_sub(1),
                            character: 0,
                        },
                    },
                    command: Some(Command::new(
                        format!(
                            "CC:{} | Cognitive:{} | Params:{} | NLOC:{} | Nesting:{}",
                            function.cyclomatic_complexity,
                            function.cognitive_complexity,
                            function.parameter_count,
                            function.nloc,
                            function.nesting_depth
                        ),
                        "rivet.metrics".to_string(),
                        None,
                    )),
                    data: None,
                })
                .collect(),
        ))
    }

    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let Some(entry) = self.cached_analysis(&uri) else {
            return Ok(None);
        };
        let Some(analysis) = entry.analysis.as_ref() else {
            return Ok(None);
        };
        let severity = self.state.config.read().await.diagnostic_severity;
        let violations = {
            let analyzer = self.state.analyzer.read().await;
            analyzer
                .check_file_thresholds(analysis)
                .into_iter()
                .filter(|violation| !is_suppressed(&entry, violation))
                .collect::<Vec<_>>()
        };

        let actions = violations
            .into_iter()
            .filter(|violation| ranges_overlap(params.range, diagnostic(violation, severity).range))
            .map(|violation| {
                let line = violation.start_line.unwrap_or(1).saturating_sub(1);
                let mut changes = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit::new(
                        Range {
                            start: Position { line, character: 0 },
                            end: Position { line, character: 0 },
                        },
                        format!(
                            "{} rivet:ignore {}\n",
                            comment_prefix(entry.language),
                            violation.metric_name
                        ),
                    )],
                );

                CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Ignore {}", violation.metric_name),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diagnostic(&violation, severity)]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        document_changes: None,
                        change_annotations: None,
                    }),
                    command: None,
                    is_preferred: Some(false),
                    disabled: None,
                    data: None,
                })
            })
            .collect::<Vec<_>>();

        Ok(Some(actions))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let Some(entry) = self.cached_analysis(&uri) else {
            return Ok(None);
        };
        let Some(analysis) = entry.analysis.as_ref() else {
            return Ok(None);
        };

        Ok(Some(
            analysis
                .functions
                .iter()
                .filter(|function| {
                    range_contains_line(params.range, function.start_line.saturating_sub(1))
                })
                .map(|function| InlayHint {
                    position: Position {
                        line: function.start_line.saturating_sub(1),
                        character: function.start_column,
                    },
                    label: InlayHintLabel::String(format!(
                        " CC {} | Cog {} | Params {} ",
                        function.cyclomatic_complexity,
                        function.cognitive_complexity,
                        function.parameter_count
                    )),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: Some(
                        format!(
                            "NLOC {} | Nesting {}",
                            function.nloc, function.nesting_depth
                        )
                        .into(),
                    ),
                    padding_left: Some(true),
                    padding_right: Some(true),
                    data: None,
                })
                .collect(),
        ))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let Some(entry) = self.cached_analysis(&uri) else {
            return Ok(None);
        };
        let Some(analysis) = entry.analysis.as_ref() else {
            return Ok(None);
        };

        Ok(Some(DocumentSymbolResponse::Nested(
            analysis
                .functions
                .iter()
                .map(|function| {
                    #[allow(deprecated)]
                    DocumentSymbol {
                        name: function.name.clone(),
                        detail: Some(format!(
                            "CC {} | Params {} | NLOC {}",
                            function.cyclomatic_complexity, function.parameter_count, function.nloc
                        )),
                        kind: SymbolKind::FUNCTION,
                        tags: None,
                        deprecated: None,
                        range: function_range(
                            function.start_line,
                            function.start_column,
                            function.end_line,
                            function.end_column,
                        ),
                        selection_range: function_range(
                            function.start_line,
                            function.start_column,
                            function.end_line,
                            function.end_column,
                        ),
                        children: None,
                    }
                })
                .collect(),
        )))
    }
}

pub async fn run_stdio(config: LspConfig) -> Result<()> {
    run_stdio_with_context(config, AnalyzerBuildContext::default()).await
}

pub async fn run_stdio_with_context(
    config: LspConfig,
    build_context: AnalyzerBuildContext,
) -> Result<()> {
    let analyzer = build_analyzer(config.clone().analyzer_config, &build_context)?;
    let state = Arc::new(BackendState::new(analyzer, config, build_context));
    let (service, socket) =
        LspService::new(move |client| RivetLanguageServer::new(client, Arc::clone(&state)));
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    Server::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}

fn initialize_capabilities(config: &LspConfig) -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(if config.analyze_on_change {
                    TextDocumentSyncKind::FULL
                } else {
                    TextDocumentSyncKind::NONE
                }),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
                ..Default::default()
            },
        )),
        hover_provider: Some(HoverProviderCapability::Simple(config.enable_hover)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        code_lens_provider: config.enable_code_lenses.then_some(CodeLensOptions {
            resolve_provider: Some(false),
        }),
        document_symbol_provider: Some(OneOf::Left(true)),
        inlay_hint_provider: Some(OneOf::Left(true)),
        ..Default::default()
    }
}

const fn function_range(
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
) -> Range {
    Range {
        start: Position {
            line: start_line.saturating_sub(1),
            character: start_column,
        },
        end: Position {
            line: end_line.saturating_sub(1),
            character: end_column,
        },
    }
}

fn diagnostic(violation: &ThresholdViolation, severity: DiagnosticSeverity) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position {
                line: violation.start_line.unwrap_or(1).saturating_sub(1),
                character: violation.start_column.unwrap_or(0),
            },
            end: Position {
                line: violation.end_line.unwrap_or(1).saturating_sub(1),
                character: violation.end_column.unwrap_or(0),
            },
        },
        severity: Some(severity.as_lsp()),
        source: Some("rivet".to_string()),
        code: Some(NumberOrString::String(violation.metric_name.clone())),
        message: format!(
            "{} exceeded {}: actual={}, threshold={}",
            violation.function_name,
            violation.metric_name,
            violation.actual_value,
            violation.threshold_value
        ),
        ..Default::default()
    }
}

const fn range_contains_line(range: Range, line: u32) -> bool {
    line >= range.start.line && line <= range.end.line
}

const fn ranges_overlap(left: Range, right: Range) -> bool {
    !(left.end.line < right.start.line || right.end.line < left.start.line)
}

fn is_suppressed(entry: &DocumentEntry, violation: &ThresholdViolation) -> bool {
    let Some(start_line) = violation.start_line else {
        return false;
    };
    let line_index = start_line.saturating_sub(1) as usize;
    let lines = entry.source.lines().collect::<Vec<_>>();
    let prefix = comment_prefix(entry.language);
    let start = line_index.saturating_sub(2);

    for line in &lines[start..line_index.min(lines.len())] {
        let trimmed = line.trim_start();
        let Some(comment) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let comment = comment.trim_start();
        let Some(rest) = comment.strip_prefix("rivet:ignore") else {
            continue;
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return true;
        }
        if rest
            .split([',', ' '])
            .filter(|value| !value.is_empty())
            .any(|metric| metric == violation.metric_name)
        {
            return true;
        }
    }

    false
}

const fn comment_prefix(language: Language) -> &'static str {
    match language {
        Language::Python | Language::Ruby => "#",
        _ => "//",
    }
}

fn find_severity(settings: &Value) -> Option<DiagnosticSeverity> {
    let severity = first_path(
        settings,
        &[
            &["lsp", "diagnostic_severity"],
            &["rivet", "lsp", "diagnostic_severity"],
            &["diagnostic_severity"],
        ],
    )
    .and_then(Value::as_str)?;
    match severity {
        "warning" => Some(DiagnosticSeverity::Warning),
        "information" => Some(DiagnosticSeverity::Information),
        _ => None,
    }
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn first_path<'a>(value: &'a Value, paths: &[&[&str]]) -> Option<&'a Value> {
    paths.iter().find_map(|path| get_path(value, path))
}

fn update_config_from_settings(settings: &Value, config: &mut LspConfig) -> bool {
    if let Some(analyze_on_change) = first_path(
        settings,
        &[
            &["lsp", "analyze_on_change"],
            &["rivet", "lsp", "analyze_on_change"],
            &["analyze_on_change"],
        ],
    )
    .and_then(Value::as_bool)
    {
        config.analyze_on_change = analyze_on_change;
    }
    if let Some(debounce_ms) = first_path(
        settings,
        &[
            &["lsp", "debounce_ms"],
            &["rivet", "lsp", "debounce_ms"],
            &["debounce_ms"],
        ],
    )
    .and_then(Value::as_u64)
    {
        config.debounce_ms = debounce_ms;
    }
    if let Some(enable_code_lenses) = first_path(
        settings,
        &[
            &["lsp", "enable_code_lenses"],
            &["rivet", "lsp", "enable_code_lenses"],
            &["enable_code_lenses"],
        ],
    )
    .and_then(Value::as_bool)
    {
        config.enable_code_lenses = enable_code_lenses;
    }
    if let Some(enable_hover) = first_path(
        settings,
        &[
            &["lsp", "enable_hover"],
            &["rivet", "lsp", "enable_hover"],
            &["enable_hover"],
        ],
    )
    .and_then(Value::as_bool)
    {
        config.enable_hover = enable_hover;
    }
    if let Some(severity) = find_severity(settings) {
        config.diagnostic_severity = severity;
    }

    let thresholds = first_path(
        settings,
        &[
            &["lsp", "thresholds"],
            &["rivet", "lsp", "thresholds"],
            &["thresholds"],
            &["rivet", "thresholds"],
        ],
    );
    if let Some(thresholds) = thresholds {
        apply_threshold_settings(&mut config.analyzer_config, thresholds);
        true
    } else {
        false
    }
}

fn apply_threshold_settings(config: &mut AnalyzerConfig, thresholds: &Value) {
    maybe_set_u32(
        thresholds,
        "max_cyclomatic_complexity",
        &mut config.thresholds.max_cyclomatic_complexity,
    );
    maybe_set_u32(
        thresholds,
        "max_cognitive_complexity",
        &mut config.thresholds.max_cognitive_complexity,
    );
    maybe_set_u32(
        thresholds,
        "max_function_length",
        &mut config.thresholds.max_function_length,
    );
    maybe_set_u32(
        thresholds,
        "max_parameter_count",
        &mut config.thresholds.max_parameter_count,
    );
    maybe_set_u32(
        thresholds,
        "max_nesting_depth",
        &mut config.thresholds.max_nesting_depth,
    );
    maybe_set_f64(
        thresholds,
        "min_maintainability_index",
        &mut config.thresholds.min_maintainability_index,
    );
}

fn maybe_set_u32(settings: &Value, key: &str, target: &mut Option<u32>) {
    if let Some(value) = settings
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
    {
        *target = Some(value);
    }
}

fn maybe_set_f64(settings: &Value, key: &str, target: &mut Option<f64>) {
    if let Some(value) = settings.get(key).and_then(Value::as_f64) {
        *target = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticSeverity, LspConfig, RivetLanguageServer, diagnostic, initialize_capabilities,
        is_suppressed, state::DocumentEntry,
    };
    use futures::StreamExt;
    use rivet_core::{Language, Severity, ThresholdViolation};
    use rivet_runtime::{AnalyzerBuildContext, build_analyzer};
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tokio::time::{Duration, timeout};
    use tower::{Service, ServiceExt};
    use tower_lsp_server::{
        LspService,
        jsonrpc::{Request, Response},
        ls_types::{
            CodeLensParams, DidChangeConfigurationParams, DidChangeTextDocumentParams,
            DidOpenTextDocumentParams, HoverParams, InitializeParams, InitializedParams,
            PartialResultParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
            TextDocumentItem, TextDocumentPositionParams, VersionedTextDocumentIdentifier,
            WorkDoneProgressParams, notification, notification::Notification,
        },
    };

    use crate::BackendState;

    fn initialize_request(id: i64) -> Request {
        Request::build("initialize")
            .params(serde_json::to_value(InitializeParams::default()).unwrap())
            .id(id)
            .finish()
    }

    async fn initialized_service() -> (
        tower_lsp_server::LspService<RivetLanguageServer>,
        tower_lsp_server::ClientSocket,
    ) {
        let config = LspConfig::default();
        let analyzer = build_analyzer(
            config.clone().analyzer_config,
            &AnalyzerBuildContext::default(),
        )
        .unwrap();
        let state = Arc::new(BackendState::new(
            analyzer,
            config,
            AnalyzerBuildContext::default(),
        ));
        let (mut service, socket) =
            LspService::new(move |client| RivetLanguageServer::new(client, Arc::clone(&state)));
        let response = service
            .ready()
            .await
            .unwrap()
            .call(initialize_request(1))
            .await
            .unwrap();
        assert_eq!(
            response,
            Some(Response::from_ok(
                1.into(),
                serde_json::to_value(super::InitializeResult {
                    capabilities: initialize_capabilities(&LspConfig::default()),
                    server_info: None,
                    ..Default::default()
                })
                .unwrap()
            ))
        );
        let _ = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("initialized")
                    .params(serde_json::to_value(InitializedParams {}).unwrap())
                    .finish(),
            )
            .await
            .unwrap();
        (service, socket)
    }

    fn open_request(uri: &str, version: i32, text: &str) -> Request {
        Request::build(notification::DidOpenTextDocument::METHOD)
            .params(
                serde_json::to_value(DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri: uri.parse().unwrap(),
                        language_id: "rust".to_string(),
                        version,
                        text: text.to_string(),
                    },
                })
                .unwrap(),
            )
            .finish()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn initialize_reports_text_sync_capabilities() {
        let request = initialize_request(99);
        let config = LspConfig::default();
        let analyzer = build_analyzer(
            config.clone().analyzer_config,
            &AnalyzerBuildContext::default(),
        )
        .unwrap();
        let state = Arc::new(BackendState::new(
            analyzer,
            config.clone(),
            AnalyzerBuildContext::default(),
        ));
        let (mut service, _) =
            LspService::new(move |client| RivetLanguageServer::new(client, Arc::clone(&state)));

        let response = service.ready().await.unwrap().call(request).await.unwrap();
        let payload = serde_json::to_value(super::InitializeResult {
            capabilities: initialize_capabilities(&config),
            server_info: None,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(response, Some(Response::from_ok(99.into(), payload)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn did_open_publishes_typed_diagnostics() {
        let (mut service, mut socket) = initialized_service().await;
        let response = service
            .ready()
            .await
            .unwrap()
            .call(open_request(
                "file:///tmp/example.rs",
                1,
                "fn too_many(a:i32,b:i32,c:i32,d:i32,e:i32,f:i32){if true && true {}}\n",
            ))
            .await
            .unwrap();
        assert_eq!(response, None);

        let notification = socket.next().await.unwrap();
        assert_eq!(
            notification.method(),
            notification::PublishDiagnostics::METHOD
        );

        let payload = serde_json::to_value(notification.params().unwrap()).unwrap();
        let diagnostics = payload["diagnostics"].as_array().unwrap();
        assert!(!diagnostics.is_empty());
        assert_eq!(
            payload["uri"],
            Value::String("file:///tmp/example.rs".to_string())
        );
    }

    #[allow(clippy::too_many_lines)]
    #[tokio::test(flavor = "current_thread")]
    async fn hover_and_code_lens_use_cached_analysis_only() {
        let config = LspConfig {
            analyze_on_change: false,
            ..LspConfig::default()
        };
        let analyzer = build_analyzer(
            config.clone().analyzer_config,
            &AnalyzerBuildContext::default(),
        )
        .unwrap();
        let state = Arc::new(BackendState::new(
            analyzer,
            config,
            AnalyzerBuildContext::default(),
        ));
        let (mut service, mut socket) =
            LspService::new(move |client| RivetLanguageServer::new(client, Arc::clone(&state)));

        let _ = service
            .ready()
            .await
            .unwrap()
            .call(initialize_request(1))
            .await
            .unwrap();
        let _ = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("initialized")
                    .params(serde_json::to_value(InitializedParams {}).unwrap())
                    .finish(),
            )
            .await
            .unwrap();

        let uri = "file:///tmp/cache.rs";
        let _ = service
            .ready()
            .await
            .unwrap()
            .call(open_request(uri, 1, "fn foo(a:i32){ if true {} }\n"))
            .await
            .unwrap();
        let _ = socket.next().await.unwrap();

        let hover_response = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("textDocument/hover")
                    .id(9)
                    .params(
                        serde_json::to_value(HoverParams {
                            text_document_position_params: TextDocumentPositionParams {
                                text_document: TextDocumentIdentifier {
                                    uri: uri.parse().unwrap(),
                                },
                                position: tower_lsp_server::ls_types::Position {
                                    line: 0,
                                    character: 1,
                                },
                            },
                            work_done_progress_params: WorkDoneProgressParams::default(),
                        })
                        .unwrap(),
                    )
                    .finish(),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(
            serde_json::to_value(&hover_response)
                .unwrap()
                .to_string()
                .contains("CC")
        );

        let lens_response = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("textDocument/codeLens")
                    .id(10)
                    .params(
                        serde_json::to_value(CodeLensParams {
                            text_document: TextDocumentIdentifier {
                                uri: uri.parse().unwrap(),
                            },
                            work_done_progress_params: WorkDoneProgressParams::default(),
                            partial_result_params: PartialResultParams::default(),
                        })
                        .unwrap(),
                    )
                    .finish(),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(
            serde_json::to_value(&lens_response)
                .unwrap()
                .to_string()
                .contains("Cognitive")
        );

        let _ = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build(notification::DidChangeTextDocument::METHOD)
                    .params(
                        serde_json::to_value(DidChangeTextDocumentParams {
                            text_document: VersionedTextDocumentIdentifier {
                                uri: uri.parse().unwrap(),
                                version: 2,
                            },
                            content_changes: vec![TextDocumentContentChangeEvent {
                                range: None,
                                range_length: None,
                                text: "fn bar(){ }\n".to_string(),
                            }],
                        })
                        .unwrap(),
                    )
                    .finish(),
            )
            .await
            .unwrap();

        let hover_after_change = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("textDocument/hover")
                    .id(11)
                    .params(
                        serde_json::to_value(HoverParams {
                            text_document_position_params: TextDocumentPositionParams {
                                text_document: TextDocumentIdentifier {
                                    uri: uri.parse().unwrap(),
                                },
                                position: tower_lsp_server::ls_types::Position {
                                    line: 0,
                                    character: 1,
                                },
                            },
                            work_done_progress_params: WorkDoneProgressParams::default(),
                        })
                        .unwrap(),
                    )
                    .finish(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&hover_after_change).unwrap()["result"],
            Value::Null
        );

        let lens_after_change = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("textDocument/codeLens")
                    .id(12)
                    .params(
                        serde_json::to_value(CodeLensParams {
                            text_document: TextDocumentIdentifier {
                                uri: uri.parse().unwrap(),
                            },
                            work_done_progress_params: WorkDoneProgressParams::default(),
                            partial_result_params: PartialResultParams::default(),
                        })
                        .unwrap(),
                    )
                    .finish(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&lens_after_change).unwrap()["result"],
            json!([])
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rapid_changes_only_publish_latest_version() {
        let config = LspConfig {
            debounce_ms: 25,
            ..LspConfig::default()
        };
        let analyzer = build_analyzer(
            config.clone().analyzer_config,
            &AnalyzerBuildContext::default(),
        )
        .unwrap();
        let state = Arc::new(BackendState::new(
            analyzer,
            config,
            AnalyzerBuildContext::default(),
        ));
        let (mut service, mut socket) =
            LspService::new(move |client| RivetLanguageServer::new(client, Arc::clone(&state)));
        let _ = service
            .ready()
            .await
            .unwrap()
            .call(initialize_request(1))
            .await
            .unwrap();
        let _ = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("initialized")
                    .params(serde_json::to_value(InitializedParams {}).unwrap())
                    .finish(),
            )
            .await
            .unwrap();

        let uri = "file:///tmp/ordering.rs";
        let _ = service
            .ready()
            .await
            .unwrap()
            .call(open_request(
                uri,
                1,
                "fn foo(a:i32){ if true && true {} }\n",
            ))
            .await
            .unwrap();
        let _ = socket.next().await.unwrap();

        for (version, text) in [
            (
                2,
                "fn foo(a:i32,b:i32,c:i32,d:i32,e:i32,f:i32){ if true && true {} }\n",
            ),
            (3, "fn foo(){ }\n"),
        ] {
            let _ = service
                .ready()
                .await
                .unwrap()
                .call(
                    Request::build(notification::DidChangeTextDocument::METHOD)
                        .params(
                            serde_json::to_value(DidChangeTextDocumentParams {
                                text_document: VersionedTextDocumentIdentifier {
                                    uri: uri.parse().unwrap(),
                                    version,
                                },
                                content_changes: vec![TextDocumentContentChangeEvent {
                                    range: None,
                                    range_length: None,
                                    text: text.to_string(),
                                }],
                            })
                            .unwrap(),
                        )
                        .finish(),
                )
                .await
                .unwrap();
        }

        let latest = timeout(Duration::from_millis(300), socket.next())
            .await
            .unwrap()
            .unwrap();
        let payload = serde_json::to_value(latest.params().unwrap()).unwrap();
        assert_eq!(payload["version"], json!(3));
        assert_eq!(payload["diagnostics"], json!([]));
        assert!(
            timeout(Duration::from_millis(100), socket.next())
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn initialize_and_settings_honor_nested_lsp_thresholds() {
        let config = LspConfig::default();
        let analyzer = build_analyzer(
            config.clone().analyzer_config,
            &AnalyzerBuildContext::default(),
        )
        .unwrap();
        let state = Arc::new(BackendState::new(
            analyzer,
            config,
            AnalyzerBuildContext::default(),
        ));
        let (mut service, mut socket) =
            LspService::new(move |client| RivetLanguageServer::new(client, Arc::clone(&state)));

        let initialize = Request::build("initialize")
            .id(1)
            .params(json!({
                "capabilities": {},
                "initializationOptions": {
                    "lsp": {
                        "enable_hover": false,
                        "enable_code_lenses": false,
                        "thresholds": {
                            "max_parameter_count": 0
                        }
                    }
                }
            }))
            .finish();
        let response = service
            .ready()
            .await
            .unwrap()
            .call(initialize)
            .await
            .unwrap()
            .unwrap();
        let payload = serde_json::to_value(response).unwrap();
        assert_eq!(
            payload["result"]["capabilities"]["hoverProvider"],
            json!(false)
        );
        assert!(payload["result"]["capabilities"]["codeLensProvider"].is_null());

        let _ = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build("initialized")
                    .params(serde_json::to_value(InitializedParams {}).unwrap())
                    .finish(),
            )
            .await
            .unwrap();
        let _ = service
            .ready()
            .await
            .unwrap()
            .call(open_request(
                "file:///tmp/config.rs",
                1,
                "fn foo(a:i32){ }\n",
            ))
            .await
            .unwrap();
        let notification = socket.next().await.unwrap();
        let payload = serde_json::to_value(notification.params().unwrap()).unwrap();
        assert_eq!(payload["version"], json!(1));
        assert!(!payload["diagnostics"].as_array().unwrap().is_empty());

        let _ = service
            .ready()
            .await
            .unwrap()
            .call(
                Request::build(notification::DidChangeConfiguration::METHOD)
                    .params(
                        serde_json::to_value(DidChangeConfigurationParams {
                            settings: json!({
                                "lsp": {
                                    "thresholds": {
                                        "max_parameter_count": 5
                                    },
                                    "enable_hover": true,
                                    "enable_code_lenses": true
                                }
                            }),
                        })
                        .unwrap(),
                    )
                    .finish(),
            )
            .await
            .unwrap();
        let reconfigured = timeout(Duration::from_millis(300), socket.next())
            .await
            .unwrap()
            .unwrap();
        let payload = serde_json::to_value(reconfigured.params().unwrap()).unwrap();
        assert_eq!(payload["version"], json!(1));
        assert_eq!(payload["diagnostics"], json!([]));
    }

    #[test]
    fn suppression_comment_disables_matching_metric() {
        let entry = DocumentEntry {
            source: "// rivet:ignore cyclomatic_complexity\nfn foo() { if true {} }\n".to_string(),
            language: Language::Rust,
            version: 1,
            revision: 1,
            dirty: false,
            analysis: None,
        };
        let violation = ThresholdViolation {
            file_path: None,
            function_name: "foo".to_string(),
            start_line: Some(2),
            start_column: Some(0),
            end_line: Some(2),
            end_column: Some(10),
            metric_name: "cyclomatic_complexity".to_string(),
            actual_value: 20.0,
            threshold_value: 15.0,
            severity: Severity::Warning,
        };

        assert!(is_suppressed(&entry, &violation));
    }

    #[test]
    fn information_severity_maps_to_lsp_information() {
        let violation = ThresholdViolation {
            file_path: None,
            function_name: "foo".to_string(),
            start_line: Some(1),
            start_column: Some(0),
            end_line: Some(1),
            end_column: Some(1),
            metric_name: "cyclomatic_complexity".to_string(),
            actual_value: 20.0,
            threshold_value: 15.0,
            severity: Severity::Warning,
        };

        assert_eq!(
            diagnostic(&violation, DiagnosticSeverity::Information).severity,
            Some(tower_lsp_server::ls_types::DiagnosticSeverity::INFORMATION)
        );
    }
}
