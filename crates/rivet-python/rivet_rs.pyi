from __future__ import annotations

from typing import Any, Mapping, Optional, Sequence

MetricValue = int | float | Mapping[str, "MetricValue"]


class HalsteadMetrics:
    n1: int
    n2: int
    big_n1: int
    big_n2: int
    vocabulary: int
    length: int
    calculated_length: float
    volume: float
    difficulty: float
    effort: float
    time: float
    bugs: float


class FileMetrics:
    nloc: int
    sloc: int
    ploc: int
    lloc: int
    cloc: int
    blank: int
    total_complexity: float
    avg_complexity: float
    max_complexity: float
    maintainability_index: float
    halstead: HalsteadMetrics
    custom_metrics: Mapping[str, MetricValue]


class FunctionAnalysis:
    name: str
    qualified_name: str
    start_line: int
    end_line: int
    start_column: int
    end_column: int
    cyclomatic_complexity: int
    cognitive_complexity: int
    parameter_count: int
    token_count: int
    nloc: int
    halstead: HalsteadMetrics
    nesting_depth: int
    custom_metrics: Mapping[str, MetricValue]


class PluginDiagnostic:
    plugin_name: str
    function_name: Optional[str]
    metric_name: Optional[str]
    message: str
    severity: str


class ParseError:
    start_line: int
    start_column: int
    end_line: int
    end_column: int
    message: str


class FileAnalysis:
    file_path: Optional[str]
    language: str
    file_metrics: FileMetrics
    functions: Sequence[FunctionAnalysis]
    plugin_diagnostics: Sequence[PluginDiagnostic]
    parse_errors: Sequence[ParseError]
    analysis_duration_ms: int


class LanguageSummary:
    files: int
    functions: int
    nloc: int


class ProjectSummary:
    total_files: int
    total_functions: int
    total_nloc: int
    avg_cyclomatic: float
    avg_cognitive: float
    avg_maintainability_index: float
    languages: Mapping[str, LanguageSummary]


class ThresholdViolation:
    file_path: Optional[str]
    function_name: str
    start_line: Optional[int]
    start_column: Optional[int]
    end_line: Optional[int]
    end_column: Optional[int]
    metric_name: str
    actual_value: float
    threshold_value: float
    severity: str


class ThresholdResult:
    passed: bool
    violations: Sequence[ThresholdViolation]


class ProjectAnalysis:
    files: Sequence[FileAnalysis]
    summary: ProjectSummary
    threshold_violations: Sequence[ThresholdViolation]


class Analyzer:
    def __init__(
        self,
        max_cyclomatic_complexity: Optional[int] = ...,
        max_cognitive_complexity: Optional[int] = ...,
        max_function_length: Optional[int] = ...,
        max_parameter_count: Optional[int] = ...,
        max_nesting_depth: Optional[int] = ...,
    ) -> None: ...

    def analyze_source(
        self,
        source: str,
        language: str,
        file_path: Optional[str] = ...,
    ) -> FileAnalysis: ...

    def analyze_directory(
        self,
        path: str,
        language: Optional[str] = ...,
    ) -> ProjectAnalysis: ...

    def check_thresholds(self, analysis: ProjectAnalysis) -> ThresholdResult: ...

    def supported_languages(self) -> Sequence[str]: ...
