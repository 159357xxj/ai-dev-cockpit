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
    /// 不透明快照 id：导出命令凭此复用同一份脱敏字节（F-07）。
    pub snapshot_id: String,
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
/// 自由文本保守上限（指令 #9：无法保证任意输入完全净化的长文本保守截断）。
const MAX_FREE_TEXT: usize = 300;

fn sanitize_and_cap(text: &str) -> String {
    let sanitized = sanitize_text(text);
    if sanitized.chars().count() <= MAX_FREE_TEXT {
        sanitized
    } else {
        let truncated: String = sanitized.chars().take(MAX_FREE_TEXT).collect();
        format!("{truncated}…[已截断]")
    }
}

/// target_id 白名单校验（fail-closed）：非预设目标的合成/污染 ID 遮蔽为
/// [未知目标]——报告标识字段不依赖 renderer 的文本规则。
fn safe_target_id(target_id: &str) -> String {
    if crate::diagnostic::targets::find_target(target_id).is_some() {
        target_id.to_string()
    } else {
        "[未知目标]".to_string()
    }
}

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
            "target_id": safe_target_id(&target.target_id),
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
                // F-02 V3 契约收敛：原始 error.summary/detail **默认不导出**
                // （不可靠净化的自由文本 fail-closed 省略；错误语义由 error_kind
                // 枚举完整承载）。
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
                "title": sanitize_and_cap(&diagnosis.title),
                "severity": diagnosis.severity,
                "confidence": diagnosis.confidence,
                "observed_facts": diagnosis.observed_facts.iter().map(|fact| sanitize_and_cap(fact)).collect::<Vec<_>>(),
                "possible_causes": diagnosis.possible_causes.iter().map(|cause| sanitize_and_cap(cause)).collect::<Vec<_>>(),
                "verification_steps": diagnosis.verification_steps.iter().map(|step| sanitize_and_cap(step)).collect::<Vec<_>>(),
                "limitations": diagnosis.limitations.iter().map(|limit| sanitize_and_cap(limit)).collect::<Vec<_>>(),
                "related_target_ids": diagnosis.related_target_ids.iter().map(|id| safe_target_id(id)).collect::<Vec<_>>(),
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
                    sanitize_and_cap(&diagnosis.title),
                    diagnosis.confidence
                ));
                for fact in &diagnosis.observed_facts {
                    markdown.push_str(&format!("- 事实：{}\n", sanitize_and_cap(fact)));
                }
                for cause in &diagnosis.possible_causes {
                    markdown.push_str(&format!("- 可能原因：{}\n", sanitize_and_cap(cause)));
                }
                for step in &diagnosis.verification_steps {
                    markdown.push_str(&format!("- 建议步骤：{}\n", sanitize_and_cap(step)));
                }
                for limit in &diagnosis.limitations {
                    markdown.push_str(&format!("- 局限：{}\n", sanitize_and_cap(limit)));
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

/// 快照状态机（F-07）：Available →（take）→ InFlight →（成功）→ Consumed /
/// （取消或写盘失败）→ Available（可重试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotState {
    Available,
    InFlight,
    Consumed,
}

/// 服务端不可变报告快照（F-07）：预览与导出复用**同一份脱敏字节**。
/// 只保存已脱敏内容（绝不缓存未脱敏数据）；id 为 UUID v4（不可预测）；
/// created_at 用单调时钟 Instant（TTL 判断与创建同基准）。
#[derive(Debug, Clone)]
pub struct ReportSnapshot {
    pub snapshot_id: String,
    pub run_id: String,
    pub format: ReportFormat,
    pub content: String,
    pub filename: String,
    pub created_at: std::time::Instant,
    pub state: SnapshotState,
}

const SNAPSHOT_TTL: std::time::Duration = std::time::Duration::from_secs(600);
const SNAPSHOT_MAX_COUNT: usize = 8;
const SNAPSHOT_MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

/// 快照管理状态（Mutex 内完成淘汰/插入/取出/状态迁移，用户取消不会破坏状态）。
#[derive(Default)]
pub struct SnapshotStore {
    available: std::collections::VecDeque<ReportSnapshot>,
    in_flight: Vec<ReportSnapshot>,
    consumed_count: usize,
}

impl SnapshotStore {
    fn total_available_bytes(&self) -> usize {
        self.available.iter().map(|item| item.content.len()).sum()
    }

    /// 淘汰过期 Available（created_at.elapsed() > TTL，单调时钟）。
    fn evict_expired(&mut self) {
        self.available
            .retain(|item| item.created_at.elapsed() < SNAPSHOT_TTL);
    }

    /// 插入新快照（Available）；先淘汰过期/超限的旧 Available。
    /// InFlight 不参与淘汰（生命周期短暂）。
    pub fn insert(&mut self, mut snapshot: ReportSnapshot) {
        self.evict_expired();
        snapshot.state = SnapshotState::Available;
        while self.available.len() >= SNAPSHOT_MAX_COUNT {
            self.available.pop_front();
        }
        while self.total_available_bytes() + snapshot.content.len() > SNAPSHOT_MAX_TOTAL_BYTES
            && !self.available.is_empty()
        {
            self.available.pop_front();
        }
        self.available.push_back(snapshot);
    }

    /// 预占快照（take）：Available → InFlight。返回快照内容副本。
    /// 并发/重复消费：InFlight 不再匹配，第二次 take 返回错误。
    pub fn take(
        &mut self,
        snapshot_id: &str,
        run_id: &str,
        format: ReportFormat,
    ) -> Result<ReportSnapshot, IpcError> {
        self.evict_expired();
        let position = self.available.iter().position(|item| {
            item.snapshot_id == snapshot_id && item.run_id == run_id && item.format == format
        });
        let Some(index) = position else {
            return Err(IpcError::new(
                IpcErrorCode::InvalidInput,
                "报告快照不存在、已过期或已使用；请重新预览后再导出",
            ));
        };
        let Some(mut snapshot) = self.available.remove(index) else {
            return Err(IpcError::new(IpcErrorCode::Internal, "快照状态不一致"));
        };
        snapshot.state = SnapshotState::InFlight;
        self.in_flight.push(snapshot.clone());
        Ok(snapshot)
    }

    /// 用户取消另存为：InFlight → Available（同一份字节可重试导出）。
    pub fn release(&mut self, snapshot_id: &str) -> Result<(), IpcError> {
        if let Some(position) = self.in_flight.iter().position(|item| {
            item.snapshot_id == snapshot_id && item.state == SnapshotState::InFlight
        }) {
            if position >= self.in_flight.len() {
                return Err(IpcError::new(IpcErrorCode::Internal, "快照状态不一致"));
            }
            let mut snapshot = self.in_flight.swap_remove(position);
            snapshot.state = SnapshotState::Available;
            self.evict_expired();
            self.available.push_back(snapshot);
            return Ok(());
        }
        Err(IpcError::new(IpcErrorCode::InvalidInput, "快照不在进行中"))
    }

    /// 保存成功：InFlight → Consumed（内容丢弃，快照失效）。
    pub fn finish_success(&mut self, snapshot_id: &str) -> Result<(), IpcError> {
        if let Some(position) = self.in_flight.iter().position(|item| {
            item.snapshot_id == snapshot_id && item.state == SnapshotState::InFlight
        }) {
            self.in_flight.remove(position);
            self.consumed_count += 1;
            return Ok(());
        }
        Err(IpcError::new(IpcErrorCode::InvalidInput, "快照不在进行中"))
    }

    /// 写盘失败：InFlight → Available（可重试）；partial_path 存在时清理部分文件。
    pub fn finish_failure(
        &mut self,
        snapshot_id: &str,
        partial_path: Option<&std::path::Path>,
    ) -> Result<(), IpcError> {
        if let Some(position) = self.in_flight.iter().position(|item| {
            item.snapshot_id == snapshot_id && item.state == SnapshotState::InFlight
        }) {
            if position >= self.in_flight.len() {
                return Err(IpcError::new(IpcErrorCode::Internal, "快照状态不一致"));
            }
            let snapshot = self.in_flight.swap_remove(position);
            self.evict_expired();
            self.available.push_back(snapshot);
            if let Some(path) = partial_path {
                let _ = std::fs::remove_file(path); // 清理可能的部分文件（尽力而为）。
            }
            return Ok(());
        }
        Err(IpcError::new(IpcErrorCode::InvalidInput, "快照不在进行中"))
    }

    pub fn available_count(&self) -> usize {
        self.available.len()
    }

    pub fn consumed_count(&self) -> usize {
        self.consumed_count
    }
}

/// 预览脱敏报告：生成一次完整脱敏内容并存为服务端不可变快照，
/// 返回预览内容与不透明 snapshot_id（后续导出复用同一份字节，F-07）。
#[tauri::command(rename_all = "snake_case")]
pub fn preview_sanitized_report(
    run_id: String,
    format: ReportFormat,
    state: State<'_, AppState>,
) -> Result<ReportPreview, IpcError> {
    let run = storage(&state)?.get_run(&run_id)?;
    let (content, filename) = build_report(&run, format);
    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let mut store = state
        .snapshot_store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    store.insert(ReportSnapshot {
        snapshot_id: snapshot_id.clone(),
        run_id: run_id.clone(),
        format,
        content: content.clone(),
        filename: filename.clone(),
        created_at: std::time::Instant::now(),
        state: SnapshotState::Available,
    });
    Ok(ReportPreview {
        run_id,
        format,
        content,
        filename,
        note: REPORT_NOTE.to_string(),
        snapshot_id,
    })
}

/// 导出脱敏报告：弹出系统原生"另存为"对话框（Rust 侧，前端不传路径——
/// 文件写入信任边界收窄为对话框结果；用户取消返回 cancelled）。
/// 写入字节与用户确认的预览**完全一致**（同一份服务端快照，F-07）；
/// 快照单次使用；无任何网络上传。
#[tauri::command(rename_all = "snake_case")]
pub fn export_sanitized_report(
    run_id: String,
    format: ReportFormat,
    snapshot_id: String,
    state: State<'_, AppState>,
) -> Result<ExportResult, IpcError> {
    let snapshot = {
        let mut store = state
            .snapshot_store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store.take(&snapshot_id, &run_id, format)?
    };

    let extension = match snapshot.format {
        ReportFormat::Json => "json",
        ReportFormat::Markdown => "md",
    };
    let Some(path) = rfd::FileDialog::new()
        .set_title("导出脱敏诊断报告")
        .set_file_name(&snapshot.filename)
        .add_filter("诊断报告", &[extension])
        .save_file()
    else {
        // 用户取消：快照回到 Available，同一份字节可再次导出。
        state
            .snapshot_store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .release(&snapshot_id)?;
        return Err(IpcError::new(IpcErrorCode::Cancelled, "已取消导出"));
    };

    // 写盘失败：清理可能的部分文件，快照回到 Available（可重试）。
    if let Err(error) = std::fs::write(&path, &snapshot.content) {
        let _ = state
            .snapshot_store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_failure(&snapshot_id, Some(&path));
        return Err(IpcError::new(
            IpcErrorCode::Internal,
            format!("写入文件失败：{error}"),
        ));
    }
    let size_bytes = snapshot.content.len() as u64;
    state
        .snapshot_store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .finish_success(&snapshot_id)?;

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

/// 测试可见别名：与 ReportSnapshot 同构（集成测试经 pub mod commands 访问）。
pub type ReportSnapshotForTest = ReportSnapshot;
