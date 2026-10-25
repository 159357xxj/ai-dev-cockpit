//! 行模型与 DTO 互转（枚举经 serde 字符串字面量往返，保持与线上契约一致）。

use serde::de::DeserializeOwned;

use crate::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, HttpOutcome, ProbeSample, ProbeType, RouteMode,
    RunStatus, TargetResult,
};

fn enum_to_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap()
        .trim_matches('"')
        .to_string()
}

fn enum_from_string<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(&format!("\"{text}\"")).map_err(|error| format!("{error}（值：{text}）"))
}

pub struct RunRow {
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub route_mode: String,
    pub repeats: u32,
    pub app_version: String,
    pub notes: Option<String>,
}

pub struct SampleRow {
    pub target_id: String,
    pub probe_type: String,
    pub route_mode: String,
    pub attempt_no: u32,
    pub success: bool,
    pub duration_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub http_outcome: Option<String>,
    pub error_kind: Option<String>,
    pub error_summary: Option<String>,
    pub error_detail: Option<String>,
    pub tested_at: String,
}

pub fn run_row_from_run(run: &DiagnosticRun) -> RunRow {
    RunRow {
        id: run.run_id.clone(),
        started_at: run.started_at.clone(),
        finished_at: run.finished_at.clone(),
        status: enum_to_string(&run.status),
        route_mode: enum_to_string(&run.route_mode),
        repeats: u32::from(run.repeats),
        app_version: run.app_version.clone(),
        notes: None,
    }
}

pub fn result_row_from_sample<'a>(
    run_id: &'a str,
    sample: &'a ProbeSample,
) -> InsertableSample<'a> {
    InsertableSample {
        run_id,
        target_id: &sample.target_id,
        probe_type: enum_to_string(&sample.probe_type),
        route_mode: enum_to_string(&sample.route_mode),
        attempt_no: sample.attempt_no,
        success: sample.success,
        duration_ms: sample.duration_ms,
        http_status: sample.http_status,
        http_outcome: sample.http_outcome.as_ref().map(enum_to_string),
        error_kind: sample
            .error
            .as_ref()
            .map(|error| enum_to_string(&error.kind)),
        error_summary: sample.error.as_ref().map(|error| error.summary.clone()),
        error_detail: sample.error.as_ref().and_then(|error| error.detail.clone()),
        tested_at: sample.tested_at.clone(),
    }
}

/// 带外键的插入行（复用 SampleRow 字段定义）。
pub struct InsertableSample<'a> {
    pub run_id: &'a str,
    pub target_id: &'a str,
    pub probe_type: String,
    pub route_mode: String,
    pub attempt_no: u32,
    pub success: bool,
    pub duration_ms: Option<u64>,
    pub http_status: Option<u16>,
    pub http_outcome: Option<String>,
    pub error_kind: Option<String>,
    pub error_summary: Option<String>,
    pub error_detail: Option<String>,
    pub tested_at: String,
}

impl RunRow {
    pub fn to_run(&self, targets: Vec<TargetResult>) -> Result<DiagnosticRun, String> {
        Ok(DiagnosticRun {
            run_id: self.id.clone(),
            started_at: self.started_at.clone(),
            finished_at: self.finished_at.clone(),
            status: enum_from_string::<RunStatus>(&self.status)?,
            route_mode: enum_from_string::<RouteMode>(&self.route_mode)?,
            repeats: self.repeats as u8,
            app_version: self.app_version.clone(),
            targets,
        })
    }
}

impl SampleRow {
    pub fn to_sample(&self) -> Result<ProbeSample, String> {
        let error = match (&self.error_kind, &self.error_summary) {
            (Some(kind), Some(summary)) => Some(DiagnosticError {
                kind: enum_from_string::<ErrorKind>(kind)?,
                summary: summary.clone(),
                detail: self.error_detail.clone(),
            }),
            _ => None,
        };
        Ok(ProbeSample {
            target_id: self.target_id.clone(),
            probe_type: enum_from_string::<ProbeType>(&self.probe_type)?,
            route_mode: enum_from_string::<RouteMode>(&self.route_mode)?,
            attempt_no: self.attempt_no,
            success: self.success,
            duration_ms: self.duration_ms,
            http_status: self.http_status,
            http_outcome: self
                .http_outcome
                .as_deref()
                .map(enum_from_string::<HttpOutcome>)
                .transpose()?,
            error,
            tested_at: self.tested_at.clone(),
        })
    }
}
