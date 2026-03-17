mod complexity;
mod halstead;
mod loc;
mod registry;

pub use complexity::total_complexity;
pub use complexity::{
    compute_cognitive_complexity, compute_cyclomatic_complexity, compute_nesting_depth,
};
pub use halstead::compute_halstead;
pub use loc::{compute_file_metrics, compute_function_nloc};
pub use registry::{MetricAnalyzer, MetricRegistry, MetricValue};
