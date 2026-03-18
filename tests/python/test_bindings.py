import json
from pathlib import Path

import pytest
from rivet_rs import Analyzer

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURE_CASES = json.loads(
    (REPO_ROOT / "tests" / "fixtures" / "binding_golden_cases.json").read_text()
)


def test_python_binding_returns_typed_analysis() -> None:
    analyzer = Analyzer(max_cyclomatic_complexity=10)
    analysis = analyzer.analyze_source(
        "def sample(value: int) -> int:\n    if value > 0:\n        return value\n    return 0\n",
        "python",
    )

    assert analysis.language == "python"
    assert analysis.functions[0].name == "sample"
    assert analysis.file_metrics.total_complexity >= 1.0


def test_python_binding_threshold_checks_use_typed_project_results() -> None:
    analyzer = Analyzer(max_cyclomatic_complexity=1)
    project = analyzer.analyze_directory("tests/fixtures/python", "python")
    threshold_result = analyzer.check_thresholds(project)

    assert threshold_result.passed is False
    assert len(threshold_result.violations) >= 1
    assert any(
        violation.metric_name == "cyclomatic_complexity"
        for violation in threshold_result.violations
    )


@pytest.mark.parametrize(
    ("fixture_path", "language", "function_name", "expected_metrics", "expected_error"),
    [
        (
            case["path"],
            case["language"],
            case.get("function_name"),
            case.get("expected_metrics"),
            case.get("expected_error"),
        )
        for case in FIXTURE_CASES
    ],
    ids=[case["language"] for case in FIXTURE_CASES],
)
def test_python_binding_fixture_regressions(
    fixture_path: str,
    language: str,
    function_name: str | None,
    expected_metrics: dict[str, int | float] | None,
    expected_error: str | None,
) -> None:
    analyzer = Analyzer()
    path = REPO_ROOT / fixture_path

    if expected_error is not None:
        with pytest.raises(RuntimeError, match=expected_error):
            analyzer.analyze_source(path.read_text(), language, str(path))
        return

    analysis = analyzer.analyze_source(path.read_text(), language, str(path))
    assert expected_metrics is not None
    assert function_name is not None

    assert analysis.language == language
    assert [function.name for function in analysis.functions] == [function_name]
    function = analysis.functions[0]
    assert function.cyclomatic_complexity == expected_metrics["cyclomatic_complexity"]
    assert function.cognitive_complexity == expected_metrics["cognitive_complexity"]
    assert function.parameter_count == expected_metrics["parameter_count"]
    assert function.nloc == expected_metrics["nloc"]
    assert function.nesting_depth == expected_metrics["nesting_depth"]
    assert analysis.file_metrics.total_complexity == expected_metrics["total_complexity"]
    assert len(analysis.parse_errors) == 0


@pytest.mark.parametrize(
    ("language", "source"),
    [
        ("javascript", "function broken(value, fallback { if (value > 0) return value; }"),
        ("go", "package sample\n\nfunc broken(value int, fallback int) int {\n\tif value > {\n\t\treturn value\n\t}\n"),
        ("csharp", "class Broken { int Sample(int value, int fallback) { if (value > ) { return value; } } }"),
        ("php", "<?php\nfunction broken($value, $fallback) {\n    if ($value > ) {\n        return $value;\n    }\n}\n"),
    ],
    ids=["javascript-parse-error", "go-parse-error", "csharp-parse-error", "php-parse-error"],
)
def test_python_binding_parser_robustness(
    language: str,
    source: str,
) -> None:
    analyzer = Analyzer()

    analysis = analyzer.analyze_source(source, language)

    assert analysis.language == language
    assert len(analysis.parse_errors) >= 1
    assert analysis.functions[0].parameter_count == 2


def test_python_binding_supported_languages_include_fixture_languages() -> None:
    analyzer = Analyzer()

    supported = set(analyzer.supported_languages())

    assert {
        case["language"] for case in FIXTURE_CASES
    }.issubset(supported)
def test_python_binding_rejects_unknown_language() -> None:
    analyzer = Analyzer()

    with pytest.raises(RuntimeError, match="unsupported language"):
        analyzer.analyze_source("fn sample() {}", "not-a-language")
