//! 脱敏报告命令（Phase 5）：本地 JSON / Markdown 导出，先预览后保存。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

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

/// 结构化安全导出模型（F-02）：**不直接序列化内部 DiagnosticRun/Diagnosis**，
/// 逐字段挑选允许导出的内容构建——自由文本（错误摘要/细节、诊断字段）全部经
/// `sanitize_text`，杜绝嵌套字段绕过脱敏的通道。保留：目标 ID、探测类型、
/// 路由模式、采样号、成功、耗时、状态码、结果分类、错误类别、统计口径。
fn secure_run_json(run: &DiagnosticRun) -> serde_json::Value {
    serde_json::json!({
        "run_id": run.run_id,
        "started_at": run.started_at,
        "finished_at": run.finished_at,
        "status": run.status,
        "route_mode": run.route_mode,
        "repeats": run.repeats,
        "app_version": run.app_version,
        "targets": run.targets.iter().map(|target| serde_json::json!({
            "target_id": target.target_id,
            "aggregates": target.aggregates.iter().map(|aggregate| serde_json::json!({
                "probe_type": aggregate.probe_type,
                "attempts_total": aggregate.attempts_total,
                "cancelled_count": aggregate.cancelled_count,
                "success_count": aggregate.success_count,
                "transport_failure_count": aggregate.transport_failure_count,
                "http_response_count": aggregate.http_response_count,
                "request_failure_rate": aggregate.request_failure_rate,
                "duration_min_ms": aggregate.duration_min_ms,
                "duration_median_ms": aggregate.duration_median_ms,
                "duration_max_ms": aggregate.duration_max_ms,
            })).collect::<Vec<_>>(),
            "samples": target.samples.iter().map(|sample| serde_json::json!({
                "probe_type": sample.probe_type,
                "route_mode": sample.route_mode,
                "attempt_no": sample.attempt_no,
                "success": sample.success,
                "duration_ms": sample.duration_ms,
                "http_status": sample.http_status,
                "http_outcome": sample.http_outcome,
                "error_kind": sample.error.as_ref().map(|error| error.kind),
                "error_summary": sample.error.as_ref().map(|error| sanitize_text(&error.summary)),
                "error_detail": sample.error.as_ref().and_then(|error| error.detail.as_deref().map(sanitize_text)),
                "tested_at": sample.tested_at,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// 诊断结论的安全化：同样逐字段（自由文本字段过 sanitize_text）。
fn secure_diagnoses_json(diagnoses: &[Diagnosis]) -> serde_json::Value {
    diagnoses
        .iter()
        .map(|diagnosis| {
            serde_json::json!({
                "rule_id": diagnosis.rule_id,
                "title": sanitize_text(&diagnosis.title),
                "severity": diagnosis.severity,
                "confidence": diagnosis.confidence,
                "observed_facts": diagnosis.observed_facts.iter().map(|fact| sanitize_text(fact)).collect::<Vec<_>>(),
                "possible_causes": diagnosis.possible_causes.iter().map(|cause| sanitize_text(cause)).collect::<Vec<_>>(),
                "verification_steps": diagnosis.verification_steps.iter().map(|step| sanitize_text(step)).collect::<Vec<_>>(),
                "limitations": diagnosis.limitations.iter().map(|limit| sanitize_text(limit)).collect::<Vec<_>>(),
                "related_target_ids": diagnosis.related_target_ids,
                "related_probe_types": diagnosis.related_probe_types,
            })
        })
        .collect::<Vec<_>>()
        .into()
}

/// 生成报告内容（纯函数，便于测试）。
pub fn build_report(run: &DiagnosticRun, format: ReportFormat) -> (String, String) {
    // 诊断在**原始**数据上求值（避免脱敏文本影响规则匹配），输出再安全化。
    let diagnoses: Vec<Diagnosis> = diagnose(std::slice::from_ref(run));
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
                "run": secure_run_json(run),
                "diagnoses": secure_diagnoses_json(&diagnoses),
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
                    diagnosis.rule_id,
                    sanitize_text(&diagnosis.title),
                    diagnosis.confidence
                ));
                for fact in &diagnosis.observed_facts {
                    markdown.push_str(&format!("- 事实：{}\n", sanitize_text(fact)));
                }
                for cause in &diagnosis.possible_causes {
                    markdown.push_str(&format!("- 可能原因：{}\n", sanitize_text(cause)));
                }
                for step in &diagnosis.verification_steps {
                    markdown.push_str(&format!("- 建议步骤：{}\n", sanitize_text(step)));
                }
                for limit in &diagnosis.limitations {
                    markdown.push_str(&format!("- 局限：{}\n", sanitize_text(limit)));
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

/// 导出脱敏报告：弹出系统原生"另存为"对话框（Rust 侧，前端不传路径——
/// 文件写入信任边界收窄为对话框结果；用户取消返回 cancelled）。
/// 写入内容为完整脱敏文本；无任何网络上传。
#[tauri::command(rename_all = "snake_case")]
pub fn export_sanitized_report(
    run_id: String,
    format: ReportFormat,
    state: State<'_, AppState>,
) -> Result<ExportResult, IpcError> {
    let run = storage(&state)?.get_run(&run_id)?;
    let (content, filename) = build_report(&run, format);

    let extension = match format {
        ReportFormat::Json => "json",
        ReportFormat::Markdown => "md",
    };
    let Some(path) = rfd::FileDialog::new()
        .set_title("导出脱敏诊断报告")
        .set_file_name(&filename)
        .add_filter("诊断报告", &[extension])
        .save_file()
    else {
        return Err(IpcError::new(IpcErrorCode::Cancelled, "已取消导出"));
    };

    std::fs::write(&path, &content)
        .map_err(|error| IpcError::new(IpcErrorCode::Internal, format!("写入文件失败：{error}")))?;
    let size_bytes = content.len() as u64;

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
