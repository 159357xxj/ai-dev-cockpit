//! 脱敏报告命令（Phase 5）：本地 JSON / Markdown 导出，先预览后保存。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::diagnostic::rules::{Diagnosis, diagnose};
use crate::diagnostic::types::{DiagnosticRun, IpcError, IpcErrorCode};
use crate::security::redaction::sanitize_text;
use crate::storage::Storage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Json,
    Markdown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ReportPreview {
    pub run_id: String,
    pub format: ReportFormat,
    pub content: String,
    pub filename: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportResult {
    pub run_id: String,
    pub format: ReportFormat,
    pub path: String,
    pub size_bytes: u64,
}

pub const REPORT_NOTE: &str = "本报告由 AI Dev Cockpit 基于有限采样（3/5 次）的只读测量生成，不构成统计显著结论；\
诊断结论为离线规则引擎的确定性输出，仅描述已观测事实与可能解释，不代表根因判定。\
报告已在本地脱敏（凭证、令牌、本地用户路径已遮蔽），导出过程无任何网络上传。";

/// 对运行与诊断做导出级脱敏：样本错误文本过 sanitize_text。
fn sanitized(run: &DiagnosticRun) -> DiagnosticRun {
    let mut run = run.clone();
    for target in &mut run.targets {
        for sample in &mut target.samples {
            if let Some(error) = &mut sample.error {
                error.summary = sanitize_text(&error.summary);
                error.detail = error.detail.as_deref().map(sanitize_text);
            }
        }
    }
    run
}

/// 生成报告内容（纯函数，便于测试）。
pub fn build_report(run: &DiagnosticRun, format: ReportFormat) -> (String, String) {
    let run = sanitized(run);
    let diagnoses: Vec<Diagnosis> = diagnose(std::slice::from_ref(&run));
    let filename = match format {
        ReportFormat::Json => format!("adc-report-{}.json", run.run_id),
        ReportFormat::Markdown => format!("adc-report-{}.md", run.run_id),
    };
    let content = match format {
        ReportFormat::Json => {
            let payload = serde_json::json!({
                "meta": {
                    "app_version": run.app_version,
                    "exported_at": crate::diagnostic::types::now_rfc3339(),
                    "note": REPORT_NOTE,
                },
                "run": run,
                "diagnoses": diagnoses,
            });
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        }
        ReportFormat::Markdown => {
            let mut markdown = String::new();
            markdown.push_str("# AI Dev Cockpit 诊断报告\n\n");
            markdown.push_str(&format!("- 运行 ID：{}\n", run.run_id));
            markdown.push_str(&format!("- 开始时间（UTC）：{}\n", run.started_at));
            markdown.push_str(&format!("- 状态：{:?}\n", run.status));
            markdown.push_str(&format!("- 路由模式：{:?}\n", run.route_mode));
            markdown.push_str(&format!("- 采样次数：{}\n", run.repeats));
            markdown.push_str(&format!("- 应用版本：{}\n", run.app_version));
            markdown.push_str(&format!("\n> {REPORT_NOTE}\n\n## 目标统计\n\n"));
            for target in &run.targets {
                markdown.push_str(&format!("### {}\n\n", target.target_id));
                for aggregate in &target.aggregates {
                    markdown.push_str(&format!(
                        "- {:?}：样本 {}，成功 {}，传输失败 {}，收到响应 {}，中位 {:?} ms\n",
                        aggregate.probe_type,
                        aggregate.attempts_total,
                        aggregate.success_count,
                        aggregate.transport_failure_count,
                        aggregate.http_response_count,
                        aggregate.duration_median_ms
                    ));
                }
                markdown.push('\n');
            }
            markdown.push_str("## 诊断结论\n\n");
            if diagnoses.is_empty() {
                markdown.push_str("无触发规则（测量均在正常范围或样本不足以诊断）。\n");
            }
            for diagnosis in &diagnoses {
                markdown.push_str(&format!(
                    "### {} {}（置信：{:?}）\n\n",
                    diagnosis.rule_id, diagnosis.title, diagnosis.confidence
                ));
                for fact in &diagnosis.observed_facts {
                    markdown.push_str(&format!("- 事实：{fact}\n"));
                }
                for cause in &diagnosis.possible_causes {
                    markdown.push_str(&format!("- 可能原因：{cause}\n"));
                }
                for step in &diagnosis.verification_steps {
                    markdown.push_str(&format!("- 建议步骤：{step}\n"));
                }
                for limit in &diagnosis.limitations {
                    markdown.push_str(&format!("- 局限：{limit}\n"));
                }
                markdown.push('\n');
            }
            markdown
        }
    };
    (content, filename)
}

fn storage<'a>(state: &'a State<'_, AppState>) -> Result<&'a Storage, IpcError> {
    state
        .storage
        .as_deref()
        .ok_or_else(|| IpcError::new(IpcErrorCode::Internal, "历史存储不可用"))
}

/// 预览脱敏报告（不落盘）。
#[tauri::command(rename_all = "snake_case")]
pub fn preview_sanitized_report(
    run_id: String,
    format: ReportFormat,
    state: State<'_, AppState>,
) -> Result<ReportPreview, IpcError> {
    let run = storage(&state)?.get_run(&run_id)?;
    let (content, filename) = build_report(&run, format);
    Ok(ReportPreview {
        run_id,
        format,
        content,
        filename,
        note: REPORT_NOTE.to_string(),
    })
}

/// 导出脱敏报告到应用数据目录的 exports/ 子目录（本地文件，无上传）。
#[tauri::command(rename_all = "snake_case")]
pub fn export_sanitized_report(
    run_id: String,
    format: ReportFormat,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ExportResult, IpcError> {
    let run = storage(&state)?.get_run(&run_id)?;
    let (content, filename) = build_report(&run, format);

    let exports_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| IpcError::new(IpcErrorCode::Internal, format!("数据目录不可用：{error}")))?
        .join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|error| {
        IpcError::new(IpcErrorCode::Internal, format!("创建导出目录失败：{error}"))
    })?;
    let path = exports_dir.join(&filename);
    let size_bytes = content.len() as u64;
    std::fs::write(&path, content)
        .map_err(|error| IpcError::new(IpcErrorCode::Internal, format!("写入文件失败：{error}")))?;

    Ok(ExportResult {
        run_id,
        format,
        path: path.to_string_lossy().to_string(),
        size_bytes,
    })
}

/// 持久化钩子（由 ProductionBackend 在运行结束时调用）。
pub fn make_persister(storage: Arc<Storage>) -> Arc<dyn Fn(&DiagnosticRun) + Send + Sync> {
    Arc::new(move |run: &DiagnosticRun| {
        if let Err(error) = storage.save_run(run) {
            eprintln!("历史持久化失败：{error}");
        }
    })
}
