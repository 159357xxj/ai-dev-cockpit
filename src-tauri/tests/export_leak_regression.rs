//! F-02 回归：导出脱敏（假凭证探针，覆盖验收报告确认的四类泄漏场景）。
//!
//! 探针数据全部为人工编造的假凭证，不接触任何真实 API Key。
//! 场景映射（RELEASE_ACCEPTANCE_REPORT §8）：
//! - Cookie 头泄漏（Json）
//! - X-API-Key / Api-Key 头泄漏（Json）
//! - 私有 IP 泄漏（Json）
//! - 敏感主机名泄漏（Json）
//!
//! 附加：多次出现、大小写变化、JSON 转义、诊断结论字段、Markdown 同策略、
//! 预览与导出一致性、无死循环/异常耗时。

use ai_dev_cockpit_lib::commands::report::{REPORT_NOTE, ReportFormat, build_report};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, ProbeAggregate, ProbeSample, ProbeType, RouteMode,
    RunStatus, TargetResult,
};
use ai_dev_cockpit_lib::security::redaction::sanitize_text;
use std::time::Instant;

/// 假凭证探针（全部人工编造，非真实凭证）。
const FAKE_COOKIE: &str = "Cookie: sessionid=FAKESESSION1234567890; token=FAKETOKEN987654321";
const FAKE_API_KEY_HEADER: &str = "X-API-Key: sk-FAKEKEY0123456789abcdef";
const FAKE_API_KEY_HEADER2: &str = "Api-Key: FAKEAPIKEYAAAAAAAABBBB";
const FAKE_PRIVATE_IP: &str = "内网网关 192.168.1.1 与 10.0.0.5 及回环 127.0.0.1";
const FAKE_INTERNAL_HOST: &str = "db.internal.corp.example 拒绝了连接";

fn contaminated_run() -> DiagnosticRun {
    let sample = |attempt: u32, summary: &str, detail: Option<&str>| ProbeSample {
        target_id: "github".to_string(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: attempt,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Unknown,
            summary: summary.to_string(),
            detail: detail.map(str::to_string),
        }),
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    };
    let samples = vec![
        sample(1, FAKE_COOKIE, None),
        sample(2, FAKE_API_KEY_HEADER, Some(FAKE_API_KEY_HEADER2)),
        sample(3, FAKE_PRIVATE_IP, None),
        sample(4, FAKE_INTERNAL_HOST, None),
    ];
    let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
    DiagnosticRun {
        run_id: "run-f02".to_string(),
        started_at: "2026-10-25T00:00:00Z".to_string(),
        finished_at: Some("2026-10-25T00:00:05Z".to_string()),
        status: RunStatus::Completed,
        route_mode: RouteMode::NoAppProxy,
        repeats: 4,
        app_version: "0.1.0".to_string(),
        targets: vec![TargetResult {
            target_id: "github".to_string(),
            samples,
            aggregates,
        }],
    }
}

/// 四类泄漏场景：JSON 与 Markdown 均不得出现任何假凭证明文。
#[test]
fn regression_f02_no_leak_in_json_and_markdown() {
    let run = contaminated_run();
    let (json, _) = build_report(&run, ReportFormat::Json);
    let (markdown, _) = build_report(&run, ReportFormat::Markdown);

    let secrets = [
        "FAKESESSION1234567890",
        "FAKETOKEN987654321",
        "sk-FAKEKEY0123456789abcdef",
        "FAKEAPIKEYAAAAAAAABBBB",
        "192.168.1.1",
        "10.0.0.5",
        "db.internal.corp.example",
    ];
    for secret in secrets {
        assert!(!json.contains(secret), "JSON 泄漏 {secret}");
        assert!(!markdown.contains(secret), "Markdown 泄漏 {secret}");
    }
    // 头名保留但值遮蔽。
    assert!(json.contains("[已隐藏]"), "敏感头值应被遮蔽");
    // 私有 IP / 内部主机遮蔽标记存在。
    assert!(json.contains("[内网地址]"));
    assert!(json.contains("[内部主机]"));
}

/// 大小写变化与多次出现。
#[test]
fn regression_f02_case_variants_and_multiple_occurrences() {
    let text = "Cookie: a=1\nCOOKIE: b=2\ncOoKiE: c=3";
    let masked = sanitize_text(text);
    assert_eq!(masked.matches("a=1").count(), 0);
    assert_eq!(masked.matches("b=2").count(), 0);
    assert_eq!(masked.matches("c=3").count(), 0);
    assert_eq!(masked.matches("[已隐藏]").count(), 3);

    let ips = "路径 10.1.2.3 然后 10.1.2.3 再 172.20.1.1";
    let masked = sanitize_text(ips);
    assert_eq!(masked.matches("10.1.2.3").count(), 0);
    assert_eq!(masked.matches("172.20.1.1").count(), 0);
}

/// JSON 转义形态（错误文本嵌在 JSON 字符串值内）。
#[test]
fn regression_f02_json_escaped_payload() {
    let run = contaminated_run();
    let (json, _) = build_report(&run, ReportFormat::Json);
    // 反查：假凭证的任何子串（含 JSON 转义后）不得出现。
    assert!(!json.contains("FAKESESSION"));
    assert!(!json.contains("sk-FAKEKEY"));
}

/// 诊断结论字段不重新泄漏（诊断文案当前为固定模板，但规则引擎未来可能
/// 携带样本摘要——安全化必须覆盖该通道）。
#[test]
fn regression_f02_diagnoses_fields_sanitized() {
    let mut run = contaminated_run();
    // 构造会触发 R04（超时）且摘要含假凭证的样本。
    run.targets[0].samples.push(ProbeSample {
        target_id: "github".to_string(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: 5,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Timeout,
            summary: format!("超时；重试头 {FAKE_API_KEY_HEADER}"),
            detail: None,
        }),
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    });
    let (json, _) = build_report(&run, ReportFormat::Json);
    let (markdown2, _) = build_report(&run, ReportFormat::Markdown);
    for output in [&json, &markdown2] {
        assert!(!output.contains("sk-FAKEKEY"), "诊断字段泄漏");
    }
}

/// 公网 IP 与诊断白名单域名保留（不误伤合法内容）。
#[test]
fn regression_f02_keeps_public_ips_and_whitelist_domains() {
    let text = "连接 8.8.8.8 与 github.com/api.openai.com 正常";
    let masked = sanitize_text(text);
    assert!(masked.contains("8.8.8.8"), "公网 IP 不应误伤：{masked}");
    assert!(masked.contains("github.com"));
    assert!(masked.contains("api.openai.com"));
    // 版本号等数字不受 IP 规则影响。
    assert_eq!(
        sanitize_text("版本 1.2.3.4 build 0.1.0"),
        "版本 1.2.3.4 build 0.1.0"
    );
}

/// 无死循环 / 异常耗时。
#[test]
fn regression_f02_no_hang_on_adversarial_input() {
    let started = Instant::now();
    let adversarial = [
        String::new(),
        "?token=".repeat(200),
        "?token=[已隐藏]&token=".repeat(100),
        "Cookie:".repeat(500),
        "192.168.".repeat(500),
        "@".repeat(1000),
        "://@://@".repeat(200),
        "\u{0}\u{1}\u{2}控制字符".to_string(),
    ];
    for payload in &adversarial {
        let _ = sanitize_text(payload);
    }
    assert!(
        started.elapsed().as_millis() < 2000,
        "脱敏异常耗时：{:?}",
        started.elapsed()
    );
}

/// 预览与导出使用同一 build_report → 内容一致（预览即所见即所得）。
#[test]
fn regression_f02_preview_matches_export_bytes() {
    let run = contaminated_run();
    let (json_content, json_name) = build_report(&run, ReportFormat::Json);
    let (md_content, md_name) = build_report(&run, ReportFormat::Markdown);
    assert!(json_name.ends_with(".json") && md_name.ends_with(".md"));
    assert!(json_content.contains(REPORT_NOTE));
    assert!(md_content.contains(REPORT_NOTE));
    // 两次调用确定性一致（除时间戳字段外的关键结构稳定）。
    let (json2, _) = build_report(&run, ReportFormat::Json);
    assert_eq!(json_content.len(), json2.len(), "报告生成应确定性");
}

// ---------------------------------------------------------------------------
// V2 独立 QA 探针场景（F-02 第二轮）——先失败后修复
// ---------------------------------------------------------------------------

/// 折叠 Cookie 行（RFC 验收探针：值经 CRLF + 空白续行延伸）。
#[test]
fn v2_folded_cookie_header() {
    let text = "Cookie: session=QA_FOLDED_COOKIE_11C\r\n  more=QA_FOLDED_MORE";
    let masked = sanitize_text(text);
    assert!(!masked.contains("QA_FOLDED_COOKIE_11C"), "{masked}");
    assert!(!masked.contains("QA_FOLDED_MORE"), "{masked}");
}

/// 多 @ URL userinfo（QA：QA_URL_PASS_12D / QA_MULTI_PASS_15G 泄漏）。
#[test]
fn v2_multi_at_userinfo() {
    for text in [
        "http://user:QA_URL_PASS_12D@proxy.example:8080",
        "http://user:QA_MULTI_PASS_15G@inner@proxy.example:8080",
        "socks5://u:QA_MULTI_PASS_15G@a@127.0.0.1:1080",
    ] {
        let masked = sanitize_text(text);
        assert!(!masked.contains("QA_URL_PASS_12D"), "{masked}");
        assert!(!masked.contains("QA_MULTI_PASS_15G"), "{masked}");
    }
}

/// 第二个及后续 Windows 用户目录路径（QA：qa-two 泄漏）。
#[test]
fn v2_multiple_user_paths() {
    let text = r"a: C:\Users\qa-one\x.conf b: C:\Users\qa-two\y.conf /home/qa-three/z";
    let masked = sanitize_text(text);
    assert!(!masked.contains("qa-one"), "{masked}");
    assert!(!masked.contains("qa-two"), "{masked}");
    assert!(!masked.contains("qa-three"), "{masked}");
}

/// Unicode 前缀的敏感查询参数（QA：İ?token=QA_UNICODE_16H → 值首字符泄漏）。
#[test]
fn v2_unicode_prefixed_query_key() {
    let text = "İ?token=QA_UNICODE_16H";
    let masked = sanitize_text(text);
    assert!(!masked.contains("QA_UNICODE_16H"), "{masked}");
    // 大写 İ（U+0130）不得破坏 UTF-8 或导致部分遮蔽。
    assert!(!masked.contains("QA_UNICODE_16"), "{masked}");
}

/// IPv6 内部地址必须整段遮蔽（QA：fd12:3456:789a::1 前缀残留）。
#[test]
fn v2_ipv6_full_address_masking() {
    for text in [
        "addr fd12:3456:789a::1 refused",
        "link fe90::1 down",
        "loop ::1 busy",
    ] {
        let masked = sanitize_text(text);
        assert!(!masked.contains("fd12:3456:789a"), "{masked}");
        assert!(!masked.contains("fe90::"), "{masked}");
        // 整段遮蔽：不允许任何十六进制段残留于遮蔽标记之前。
        assert!(!masked.contains("fd12"), "{masked}");
        assert!(!masked.contains("fe90"), "{masked}");
    }
}

/// 错误摘要污染 R04 observed_facts 并进入两种报告（QA 场景）。
#[test]
fn v2_r04_facts_do_not_carry_raw_error_summary() {
    let sample = |attempt: u32| ProbeSample {
        target_id: "github".into(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: attempt,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Timeout,
            summary: "超时 Cookie: QA_R04_COOKIE_99Z".into(),
            detail: None,
        }),
        tested_at: "2026-10-25T00:00:00Z".into(),
    };
    let samples = vec![sample(1), sample(2), sample(3)];
    let aggregates = vec![ProbeAggregate::from_samples(ProbeType::Https, &samples)];
    let run = DiagnosticRun {
        run_id: "run-r04".into(),
        started_at: "2026-10-25T00:00:00Z".into(),
        finished_at: Some("2026-10-25T00:00:05Z".into()),
        status: RunStatus::Completed,
        route_mode: RouteMode::NoAppProxy,
        repeats: 3,
        app_version: "0.1.0".into(),
        targets: vec![TargetResult {
            target_id: "github".into(),
            samples,
            aggregates,
        }],
    };
    let (json, _) = build_report(&run, ReportFormat::Json);
    let (md, _) = build_report(&run, ReportFormat::Markdown);
    for output in [&json, &md] {
        assert!(
            !output.contains("QA_R04_COOKIE_99Z"),
            "R04 事实携带原始摘要：{output}"
        );
    }
}

// ---------------------------------------------------------------------------
// F-02 V2 性质测试（随机/对抗输入：无 panic、无死循环、长度有界、已知秘密不漏）
// ---------------------------------------------------------------------------

/// 确定性伪随机（LCG，无 rand 依赖）：生成混合字符集字符串。
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick(&mut self, items: &[char]) -> char {
        items[(self.next() % items.len() as u64) as usize]
    }
}

const CHARSET: &[char] = &[
    'a', 'B', '9', ':', '/', '@', '"', '?', '&', '=', '.', '-', ' ', '\n', '\r', '\t',
    // 多字节字符（含大小写展开差异的 İ U+0130、CJK、emoji）。
    'İ', 'ı', '中', '文', '🎉', 'ß', 'Ω', 'å', // 结构字符。
    '{', '}', '[', ']', ',', '\'', '\\', ';',
];

#[test]
fn property_no_panic_bounded_length_no_releak() {
    let mut rng = Lcg(0xA1C0_CAFE_F00D);
    let mut total_checks = 0usize;
    for round in 0..300 {
        let length = (rng.next() % 400) as usize;
        let mut input = String::new();
        for _ in 0..length {
            input.push(rng.pick(CHARSET));
        }
        // 每若干轮注入已知假秘密（覆盖"已知敏感内容不重漏"性质）。
        let injected = round % 5 == 0;
        if injected {
            input.push_str("Cookie: QA_PROP_COOKIE_XY7");
            input.push_str("http://u:QA_PROP_PASS_ZZ9@host.internal");
            input.push_str(r"192.168.77.7 fd00::1 C:\Users\qa-prop");
        }
        let started = std::time::Instant::now();
        let output = sanitize_text(&input);
        // 1) 无死循环/异常耗时。
        assert!(
            started.elapsed().as_millis() < 500,
            "round {round} 耗时异常"
        );
        // 2) 无 panic 已由执行证明；输出是合法 String（类型保证 UTF-8 有效）。
        // 3) 长度上界：遮蔽标记扩张有限（每字符最多触发有限规则，×16 上界足够宽松）。
        assert!(
            output.len() <= input.len() * 16 + 4096,
            "round {round} 输出异常膨胀：{} → {}",
            input.len(),
            output.len()
        );
        // 4) 已知秘密不重漏。
        if injected {
            for secret in [
                "QA_PROP_COOKIE_XY7",
                "QA_PROP_PASS_ZZ9",
                "192.168.77.7",
                "fd00::1",
                "qa-prop",
            ] {
                assert!(
                    !output.contains(secret),
                    "round {round} 泄漏 {secret}：{output}"
                );
            }
        }
        total_checks += 1;
    }
    assert_eq!(total_checks, 300);
}

/// 纯随机字节串经 UTF-8 安全化后同样不 panic（含非法 UTF-8 替换字符路径）。
#[test]
fn property_random_bytes_lossy_safe() {
    let mut rng = Lcg(42);
    for _ in 0..100 {
        let length = (rng.next() % 200) as usize;
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            bytes.push((rng.next() % 256) as u8);
        }
        let text = String::from_utf8_lossy(&bytes);
        let output = sanitize_text(&text);
        assert!(output.len() <= text.len() * 16 + 4096);
    }
}

/// 长自由文本保守截断（指令 #9）。
#[test]
fn property_long_free_text_truncated_in_export() {
    // 秘密置于 300 字符截断点之后（前 350 字节为无害填充），
    // 单独测 cap 通道；Header 形态的遮蔽已由其它测试覆盖。
    let mut summary = "x".repeat(350);
    summary.push_str("SECRET_QA_LONG_1_PLAIN token=QA_LONG_2 10.20.30.40");
    let sample = ProbeSample {
        target_id: "github".into(),
        probe_type: ProbeType::Https,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: 1,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: Some(DiagnosticError {
            kind: ErrorKind::Timeout,
            summary,
            detail: None,
        }),
        tested_at: "2026-10-25T00:00:00Z".into(),
    };
    let aggregates = vec![ProbeAggregate::from_samples(
        ProbeType::Https,
        std::slice::from_ref(&sample),
    )];
    let run = DiagnosticRun {
        run_id: "run-cap".into(),
        started_at: "t".into(),
        finished_at: None,
        status: RunStatus::Completed,
        route_mode: RouteMode::NoAppProxy,
        repeats: 1,
        app_version: "0".into(),
        targets: vec![TargetResult {
            target_id: "github".into(),
            samples: vec![sample],
            aggregates,
        }],
    };
    let (json, _) = build_report(&run, ReportFormat::Json);
    assert!(
        json.contains("[已截断]"),
        "超过 300 字符的自由文本应被保守截断"
    );
    assert!(
        !json.contains("SECRET_QA_LONG_1_PLAIN"),
        "截断点后的秘密不得出现"
    );
    assert!(!json.contains("QA_LONG_2"), "截断+脱敏后不得残留秘密");
    assert!(!json.contains("10.20.30.40"), "内网 IP 不得残留");
}

// ---------------------------------------------------------------------------
// F-07：预览与导出复用同一份服务端快照字节
// ---------------------------------------------------------------------------

#[test]
fn regression_f07_snapshot_bytes_equal_preview_and_survive_history_change() {
    use ai_dev_cockpit_lib::commands::report::SnapshotStore;
    use std::sync::{Arc, Mutex};

    let store: Arc<Mutex<SnapshotStore>> = Arc::new(Mutex::new(SnapshotStore::default()));

    // 第一次生成（模拟 preview IPC 内部逻辑）。
    let run = contaminated_run();
    let (content1, filename) = build_report(&run, ReportFormat::Json);
    let snapshot_id = "snap-abc-123".to_string();
    store.lock().unwrap().insert(
        ai_dev_cockpit_lib::commands::report::ReportSnapshotForTest {
            snapshot_id: snapshot_id.clone(),
            run_id: run.run_id.clone(),
            format: ReportFormat::Json,
            content: content1.clone(),
            filename: filename.clone(),
            created_at: std::time::Instant::now(),
            exported: false,
        },
    );

    // 间隔一段时间（QA 场景：15ms 后导出）。
    std::thread::sleep(std::time::Duration::from_millis(15));

    // 模拟"历史数据变化"：原始 run 被修改后重新 build（若导出走重生成路径，
    // exported_at/内容会不同——快照路径则必须仍返回第一份字节）。
    let mut changed = run.clone();
    changed.targets[0].samples[0].error = Some(DiagnosticError {
        kind: ErrorKind::Unknown,
        summary: "QA_CHANGED_AFTER_PREVIEW".into(),
        detail: None,
    });
    let (regenerated, _) = build_report(&changed, ReportFormat::Json);
    assert_ne!(regenerated, content1, "重生成内容确实不同（前提成立）");

    // 通过快照取出：必须与预览字节**完全一致**，不受历史变化影响。
    let snapshot = {
        let mut guard = store.lock().unwrap();
        // 模拟 export 命令的 take 逻辑（按 id/run/format 校验）。
        let position = guard
            .snapshots
            .iter()
            .position(|item| {
                item.snapshot_id == snapshot_id
                    && item.run_id == run.run_id
                    && item.format == ReportFormat::Json
                    && !item.exported
            })
            .expect("有效快照应可取出");
        guard.snapshots.remove(position).expect("有效快照应可取出")
    };
    assert_eq!(
        snapshot.content, content1,
        "导出字节必须与预览完全一致（F-07）"
    );

    // 单次使用：再次取同 id → 不存在。
    let second_take = store
        .lock()
        .unwrap()
        .snapshots
        .iter()
        .any(|item| item.snapshot_id == snapshot_id);
    assert!(!second_take, "快照取出后即失效（防重复导出）");
}

#[test]
fn regression_f07_invalid_snapshot_rejected() {
    use ai_dev_cockpit_lib::commands::report::SnapshotStore;
    let mut store = SnapshotStore::default();
    let error = store
        .take("nonexistent", "run-x", ReportFormat::Json)
        .err()
        .expect("无效快照必须被拒绝");
    assert!(error.message.contains("不存在") || error.message.contains("过期"));
}

#[test]
fn regression_f07_snapshot_eviction_limits() {
    use ai_dev_cockpit_lib::commands::report::SnapshotStore;
    let mut store = SnapshotStore::default();
    // 超过数量上限：最旧的被淘汰。
    for index in 0..12 {
        store.insert(
            ai_dev_cockpit_lib::commands::report::ReportSnapshotForTest {
                snapshot_id: format!("snap-{index}"),
                run_id: format!("run-{index}"),
                format: ReportFormat::Json,
                content: "x".repeat(100),
                filename: format!("f{index}.json"),
                created_at: std::time::Instant::now(),
                exported: false,
            },
        );
    }
    let count = store.snapshots.len();
    assert!(count <= 8, "快照数量上限 8，实际 {count}");
    // 最旧的不在队列。
    assert!(
        !store
            .snapshots
            .iter()
            .any(|item| item.snapshot_id == "snap-0")
    );
}
