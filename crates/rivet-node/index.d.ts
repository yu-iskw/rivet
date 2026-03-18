export type MetricValue = number | { [key: string]: MetricValue };

export interface AnalyzerOptions {
  maxCyclomaticComplexity?: number;
  maxCognitiveComplexity?: number;
  maxFunctionLength?: number;
  maxParameterCount?: number;
  maxNestingDepth?: number;
}

export interface HalsteadMetrics {
  n1: number;
  n2: number;
  bigN1: number;
  bigN2: number;
  vocabulary: number;
  length: number;
  calculatedLength: number;
  volume: number;
  difficulty: number;
  effort: number;
  time: number;
  bugs: number;
}

export interface FileMetrics {
  nloc: number;
  sloc: number;
  ploc: number;
  lloc: number;
  cloc: number;
  blank: number;
  totalComplexity: number;
  avgComplexity: number;
  maxComplexity: number;
  maintainabilityIndex: number;
  halstead: HalsteadMetrics;
  customMetrics: Record<string, MetricValue>;
}

export interface FunctionAnalysis {
  name: string;
  qualifiedName: string;
  startLine: number;
  endLine: number;
  startColumn: number;
  endColumn: number;
  cyclomaticComplexity: number;
  cognitiveComplexity: number;
  parameterCount: number;
  tokenCount: number;
  nloc: number;
  halstead: HalsteadMetrics;
  nestingDepth: number;
  customMetrics: Record<string, MetricValue>;
}

export interface PluginDiagnostic {
  pluginName: string;
  functionName?: string | null;
  metricName?: string | null;
  message: string;
  severity: string;
}

export interface ParseError {
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
  message: string;
}

export interface FileAnalysis {
  filePath?: string | null;
  language: string;
  fileMetrics: FileMetrics;
  functions: FunctionAnalysis[];
  pluginDiagnostics: PluginDiagnostic[];
  parseErrors: ParseError[];
  analysisDurationMs: number;
}

export interface LanguageSummary {
  files: number;
  functions: number;
  nloc: number;
}

export interface ProjectSummary {
  totalFiles: number;
  totalFunctions: number;
  totalNloc: number;
  avgCyclomatic: number;
  avgCognitive: number;
  avgMaintainabilityIndex: number;
  languages: Record<string, LanguageSummary>;
}

export interface ThresholdViolation {
  filePath?: string | null;
  functionName: string;
  startLine?: number | null;
  startColumn?: number | null;
  endLine?: number | null;
  endColumn?: number | null;
  metricName: string;
  actualValue: number;
  thresholdValue: number;
  severity: string;
}

export interface ThresholdResult {
  passed: boolean;
  violations: ThresholdViolation[];
}

export interface ProjectAnalysis {
  files: FileAnalysis[];
  summary: ProjectSummary;
  thresholdViolations: ThresholdViolation[];
}

export declare class JsAnalyzer {
  constructor(options?: AnalyzerOptions);
  analyzeSource(
    source: string,
    language: string,
    filePath?: string,
  ): FileAnalysis;
  analyzeDirectory(path: string, language?: string): Promise<ProjectAnalysis>;
  checkThresholds(analysis: ProjectAnalysis): ThresholdResult;
  supportedLanguages(): string[];
}
