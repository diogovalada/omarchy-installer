use crate::{
    CancelRequest, CancelResult, CapabilitiesRequest, CapabilitiesResponse, DiagnosticBundle,
    DiagnosticsRequest, ExecuteRequest, OperationReceipt, PlanRequest, PlanResponse,
    PreflightRequest, PreflightResponse, ProbeRequest, ProbeResponse, ProgressEvent,
    ProviderDescriptor, ProviderError, RecoverRequest, RecoveryResult, ResumeRequest,
    ResumeResponse,
};

/// Receives ordered progress without prescribing threads, async runtimes, IPC, or processes.
pub trait ProgressSink {
    fn emit(&mut self, event: ProgressEvent) -> Result<(), ProviderError>;
}

impl<F> ProgressSink for F
where
    F: FnMut(ProgressEvent) -> Result<(), ProviderError>,
{
    fn emit(&mut self, event: ProgressEvent) -> Result<(), ProviderError> {
        self(event)
    }
}

/// The complete lifecycle implemented by Try, Direct, and media providers.
///
/// Transport and privilege boundaries are intentionally outside this trait. Implementations
/// return typed data and must not receive command lines, executable paths, or untyped maps.
pub trait ProviderLifecycle {
    fn descriptor(&self) -> ProviderDescriptor;

    fn probe(&self, request: ProbeRequest) -> Result<ProbeResponse, ProviderError>;

    fn capabilities(
        &self,
        request: CapabilitiesRequest,
    ) -> Result<CapabilitiesResponse, ProviderError>;

    fn preflight(&self, request: PreflightRequest) -> Result<PreflightResponse, ProviderError>;

    fn plan(&self, request: PlanRequest) -> Result<PlanResponse, ProviderError>;

    fn execute(
        &mut self,
        request: ExecuteRequest,
        progress: &mut dyn ProgressSink,
    ) -> Result<OperationReceipt, ProviderError>;

    fn resume(&self, request: ResumeRequest) -> Result<ResumeResponse, ProviderError>;

    fn recover(&mut self, request: RecoverRequest) -> Result<RecoveryResult, ProviderError>;

    fn cancel(&mut self, request: CancelRequest) -> Result<CancelResult, ProviderError>;

    fn diagnostics(&self, request: DiagnosticsRequest) -> Result<DiagnosticBundle, ProviderError>;
}
