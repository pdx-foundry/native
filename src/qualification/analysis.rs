/// Static composition inputs contain no content or live-tool prerequisites.
#[derive(Debug, Clone)]
pub(crate) struct AnalysisInputs {
    pub composition: String,
    pub executable: String,
    pub slice: String,
    pub implementation: String,
    pub method: &'static str,
    pub decoder: &'static str,
}
