//! 规则诊断引擎全量表驱动测试（Phase 4）。
//!
//! 全部使用固定 fixture，离线、确定性；覆盖 12 条规则的触发/不触发、
//! 冲突去重、置信分层、取消口径（D-014）与禁止断言扫描。

use ai_dev_cockpit_lib::diagnostic::rules::{Confidence, Diagnosis, Severity, diagnose};
use ai_dev_cockpit_lib::diagnostic::types::{
    DiagnosticError, DiagnosticRun, ErrorKind, HttpOutcome, ProbeAggregate, ProbeSample, ProbeType,
    RouteMode, RunStatus, TargetResult,
};

// ---------------------------------------------------------------------------
// fixture 构造
// ---------------------------------------------------------------------------

const TARGET: &str = "github";

fn base_sample(probe: ProbeType, attempt: u32) -> ProbeSample {
    ProbeSample {
        target_id: TARGET.to_string(),
        probe_type: probe,
        route_mode: RouteMode::NoAppProxy,
        attempt_no: attempt,
        success: false,
        duration_ms: None,
        http_status: None,
        http_outcome: None,
        error: None,
        tested_at: "2026-10-25T00:00:00Z".to_string(),
    }
}

fn ok_sample(mut sample: ProbeSample, duration: u64) -> ProbeSample {
    sample.success = true;
    sample.duration_ms = Some(duration);
    if sample.probe_type == ProbeType::Https {
        sample.http_status = Some(200);
        sample.http_outcome = Some(HttpOutcome::Ok2xx);
    }
    sample
}

fn transport(mut sample: ProbeSample, kind: ErrorKind) -> ProbeSample {
    sample.success = false;
    sample.error = Some(DiagnosticError {
        kind,
        summary: format!("模拟 {kind:?}"),
        detail: None,
    });
    sample
}

fn status_sample(mut sample: ProbeSample, status: u16) -> ProbeSample {
    sample.success = (200..300).contains(&status);
    sample.duration_ms = if sample.success { Some(400) } else { None };
    sample.http_status = Some(status);
    sample.http_outcome = Some(match status {
        200..=299 => HttpOutcome::Ok2xx,
        401 | 403 => HttpOutcome::AuthRequired401To403,
        407 => HttpOutcome::ProxyAuth407,
        429 => HttpOutcome::RateLimited429,
        500..=599 => HttpOutcome::ServerError5xx,
        _ => HttpOutcome::ClientErrorOther4xx,
    });
    sample
}

fn https(attempt: u32) -> ProbeSample {
    base_sample(ProbeType::Https, attempt)
}

fn recompute(samples: &[ProbeSample]) -> Vec<ProbeAggregate> {
    let mut aggregates = Vec::new();
    for probe_type in [ProbeType::Dns, ProbeType::Tcp, ProbeType::Https] {
        let subset: Vec<ProbeSample> = samples
            .iter()
            .filter(|sample| sample.probe_type == probe_type)
            .cloned()
            .collect();
        if !subset.is_empty() {
            aggregates.push(ProbeAggregate::from_samples(probe_type, &subset));
        }
    }
    aggregates
}

fn make_run(
    route_mode: RouteMode,
    status: RunStatus,
    repeats: u8,
    samples: Vec<ProbeSample>,
) -> DiagnosticRun {
    let mut samples = samples;
    for sample in &mut samples {
        sample.target_id = TARGET.to_string();
        sample.route_mode = route_mode;
    }
    DiagnosticRun {
        run_id: "run-fixture".to_string(),
        started_at: "2026-10-25T00:00:00Z".to_string(),
        finished_at: Some("2026-10-25T00:00:05Z".to_string()),
        status,
        route_mode,
        repeats,
        app_version: "0.1.0".to_string(),
        targets: vec![TargetResult {
            target_id: TARGET.to_string(),
            samples: samples.clone(),
            aggregates: recompute(&samples),
        }],
    }
}

/// 基线正常（DNS/TCP 成功）+ 指定 HTTPS 样本的完整运行。
fn run_with_https(route_mode: RouteMode, https_samples: Vec<ProbeSample>) -> DiagnosticRun {
    let mut samples = vec![
        ok_sample(base_sample(ProbeType::Dns, 1), 12),
        ok_sample(base_sample(ProbeType::Tcp, 1), 40),
    ];
    samples.extend(https_samples);
    make_run(route_mode, RunStatus::Completed, 3, samples)
}

fn rule_ids(diagnoses: &[Diagnosis]) -> Vec<String> {
    diagnoses.iter().map(|item| item.rule_id.clone()).collect()
}

fn find_rule<'a>(diagnoses: &'a [Diagnosis], rule: &str) -> Option<&'a Diagnosis> {
    diagnoses.iter().find(|item| item.rule_id == rule)
}

// ---------------------------------------------------------------------------
// R01..R09
// ---------------------------------------------------------------------------

#[test]
fn r01_dns_failure_fires_and_merges_https_dns_evidence() {
    let mut samples = vec![
        transport(base_sample(ProbeType::Dns, 1), ErrorKind::DnsFailure),
        ok_sample(base_sample(ProbeType::Tcp, 1), 40),
    ];
    samples.extend(vec![transport(https(1), ErrorKind::DnsFailure); 3]);
    let run = make_run(RouteMode::NoAppProxy, RunStatus::Completed, 3, samples);

    let out = diagnose(&[run]);
    let r01 = find_rule(&out, "R01").expect("DNS 失败应触发 R01");
    assert_eq!(r01.confidence, Confidence::EvidenceSufficient);
    assert!(r01.observed_facts.iter().any(|fact| fact.contains("HTTPS")));
    // 去重：仅一个 R01
    assert_eq!(out.iter().filter(|item| item.rule_id == "R01").count(), 1);
}

#[test]
fn r02_tcp_failure_fires() {
    let mut samples = vec![
        ok_sample(base_sample(ProbeType::Dns, 1), 12),
        transport(base_sample(ProbeType::Tcp, 1), ErrorKind::TcpFailure),
    ];
    samples.extend(vec![transport(https(1), ErrorKind::TcpFailure); 3]);
    let run = make_run(RouteMode::NoAppProxy, RunStatus::Completed, 3, samples);
    let out = diagnose(&[run]);
    assert!(
        find_rule(&out, "R02").is_some(),
        "rules: {:?}",
        rule_ids(&out)
    );
}

#[test]
fn r03_tls_all_fail_evidence_sufficient_partial_possible() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![transport(https(1), ErrorKind::TlsFailure); 3],
    );
    let out = diagnose(&[run]);
    let r03 = find_rule(&out, "R03").expect("全部 TLS 失败应触发");
    assert_eq!(r03.confidence, Confidence::EvidenceSufficient);

    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            transport(https(2), ErrorKind::TlsFailure),
            ok_sample(https(3), 400),
        ],
    );
    let out = diagnose(&[run]);
    assert_eq!(
        find_rule(&out, "R03")
            .expect("部分 TLS 失败应触发")
            .confidence,
        Confidence::Possible
    );
}

#[test]
fn r04_timeout_all_and_partial() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![transport(https(1), ErrorKind::Timeout); 3],
    );
    let out = diagnose(&[run]);
    assert_eq!(
        find_rule(&out, "R04").expect("全部超时应触发").confidence,
        Confidence::EvidenceSufficient
    );

    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 500),
            transport(https(2), ErrorKind::Timeout),
            ok_sample(https(3), 500),
        ],
    );
    let out = diagnose(&[run]);
    assert_eq!(
        find_rule(&out, "R04").expect("部分超时应触发").confidence,
        Confidence::Possible
    );
}

#[test]
fn r05_401_403_neutral_never_claims_availability() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            status_sample(https(1), 401),
            status_sample(https(2), 403),
            ok_sample(https(3), 200),
        ],
    );
    let out = diagnose(&[run]);
    let r05 = find_rule(&out, "R05").expect("401/403 应触发");
    assert_eq!(r05.severity, Severity::Info);
    assert_eq!(r05.confidence, Confidence::EvidenceSufficient);
    for cause in &r05.possible_causes {
        assert!(!cause.contains("正常登录"), "{cause}");
        assert!(!cause.contains("业务正常"), "{cause}");
        assert!(!cause.contains("账号"), "{cause}");
    }
    assert!(!r05.title.contains("正常"));
    // 事实只描述观测
    assert!(r05.observed_facts[0].contains("收到"));
}

#[test]
fn r06_407_fires() {
    let run = run_with_https(
        RouteMode::ManualLocalProxy,
        vec![status_sample(https(1), 407); 3],
    );
    let out = diagnose(&[run]);
    let r06 = find_rule(&out, "R06").expect("407 应触发");
    assert!(
        r06.limitations
            .iter()
            .any(|item| item.contains("不发送任何代理凭证"))
    );
}

#[test]
fn r07_429_fires_without_ip_reputation_claims() {
    let run = run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 429); 3]);
    let out = diagnose(&[run]);
    let r07 = find_rule(&out, "R07").expect("429 应触发");
    assert!(r07.limitations.iter().any(|item| item.contains("信誉")));
}

#[test]
fn r08_5xx_partial_and_full() {
    let run = run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 500); 3]);
    let out = diagnose(&[run]);
    assert_eq!(
        find_rule(&out, "R08").expect("全部 5xx").confidence,
        Confidence::EvidenceSufficient
    );

    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 300),
            ok_sample(https(2), 300),
            status_sample(https(3), 502),
        ],
    );
    let out = diagnose(&[run]);
    assert_eq!(
        find_rule(&out, "R08").expect("部分 5xx").confidence,
        Confidence::Possible
    );
}

#[test]
fn r09_manual_proxy_unavailable_critical_and_mode_gated() {
    let run = run_with_https(
        RouteMode::ManualLocalProxy,
        vec![transport(https(1), ErrorKind::ProxyUnavailable); 3],
    );
    let out = diagnose(&[run]);
    let r09 = find_rule(&out, "R09").expect("manual 模式死代理应触发");
    assert_eq!(r09.severity, Severity::Critical);
    assert!(r09.limitations.iter().any(|item| item.contains("绝不回落")));

    // 非 manual 模式不触发（模式门控）
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![transport(https(1), ErrorKind::ProxyUnavailable); 3],
    );
    assert!(find_rule(&diagnose(&[run]), "R09").is_none());
}

// ---------------------------------------------------------------------------
// R10 跨运行
// ---------------------------------------------------------------------------

#[test]
fn r10_fires_on_divergent_paths_and_ignores_cancelled_runs() {
    let good = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            ok_sample(https(2), 400),
            ok_sample(https(3), 400),
        ],
    );
    let bad = run_with_https(
        RouteMode::ManualLocalProxy,
        vec![transport(https(1), ErrorKind::ProxyUnavailable); 3],
    );
    let out = diagnose(&[good.clone(), bad.clone()]);
    let r10 = find_rule(&out, "R10").expect("路径结果不一致应触发");
    assert_eq!(r10.confidence, Confidence::Possible);
    assert!(
        r10.limitations
            .iter()
            .any(|item| item.contains("节点性能排名"))
    );
    assert!(
        r10.limitations
            .iter()
            .any(|item| item.contains("no_app_proxy"))
    );

    // 路径一致 → 不触发
    let good2 = run_with_https(
        RouteMode::ManualLocalProxy,
        vec![
            ok_sample(https(1), 400),
            ok_sample(https(2), 400),
            ok_sample(https(3), 400),
        ],
    );
    let out = diagnose(&[good.clone(), good2]);
    assert!(find_rule(&out, "R10").is_none());

    // 取消任务不参与判断
    let mut cancelled = bad;
    cancelled.status = RunStatus::Cancelled;
    let out = diagnose(&[good, cancelled]);
    assert!(find_rule(&out, "R10").is_none(), "取消任务不得参与 R10");
}

// ---------------------------------------------------------------------------
// R11
// ---------------------------------------------------------------------------

#[test]
fn r11_baseline_ok_but_https_all_failed() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![transport(https(1), ErrorKind::TlsFailure); 3],
    );
    let out = diagnose(&[run]);
    let r11 = find_rule(&out, "R11").expect("基线正常但 HTTPS 全失败应触发");
    assert_eq!(r11.confidence, Confidence::Possible);
    assert!(r11.limitations.iter().any(|item| item.contains("强行归因")));
}

#[test]
fn r11_not_fired_when_https_partially_succeeds_or_baseline_fails() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            transport(https(2), ErrorKind::TlsFailure),
            ok_sample(https(3), 400),
        ],
    );
    assert!(find_rule(&diagnose(&[run]), "R11").is_none());

    let samples = vec![
        ok_sample(base_sample(ProbeType::Dns, 1), 12),
        transport(base_sample(ProbeType::Tcp, 1), ErrorKind::TcpFailure),
        transport(https(1), ErrorKind::TlsFailure),
        transport(https(2), ErrorKind::TlsFailure),
        transport(https(3), ErrorKind::TlsFailure),
    ];
    let run = make_run(RouteMode::NoAppProxy, RunStatus::Completed, 3, samples);
    assert!(find_rule(&diagnose(&[run]), "R11").is_none());
}

// ---------------------------------------------------------------------------
// R12
// ---------------------------------------------------------------------------

#[test]
fn r12_all_cancelled_insufficient_and_no_failure_rules() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            transport(https(1), ErrorKind::Cancelled),
            transport(https(2), ErrorKind::Cancelled),
            transport(https(3), ErrorKind::Cancelled),
        ],
    );
    let mut cancelled = run;
    cancelled.status = RunStatus::Cancelled;
    let out = diagnose(&[cancelled]);
    let r12 = find_rule(&out, "R12").expect("全部取消应触发 R12");
    assert_eq!(r12.confidence, Confidence::Insufficient);
    // 取消样本不计入网络失败（D-014）：R03/R04 不得因取消触发
    assert!(find_rule(&out, "R03").is_none());
    assert!(find_rule(&out, "R04").is_none());
}

#[test]
fn r12_partial_cancellation_and_missing_samples() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            ok_sample(https(2), 400),
            transport(https(3), ErrorKind::Cancelled),
        ],
    );
    let mut cancelled = run;
    cancelled.status = RunStatus::Cancelled;
    let out = diagnose(&[cancelled]);
    assert!(find_rule(&out, "R12").is_some());
    assert!(find_rule(&out, "R03").is_none());

    // 计划 5 执行 3 → 样本不足
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            ok_sample(https(2), 400),
            ok_sample(https(3), 400),
        ],
    );
    let mut short_run = run;
    short_run.repeats = 5;
    assert!(
        find_rule(&diagnose(&[short_run]), "R12").is_some(),
        "计划 5 执行 3 应提示样本不足"
    );
}

// ---------------------------------------------------------------------------
// 跨场景组合
// ---------------------------------------------------------------------------

#[test]
fn dns_failure_but_manual_proxy_https_success_is_not_contradictory() {
    let samples = vec![
        transport(base_sample(ProbeType::Dns, 1), ErrorKind::DnsFailure),
        ok_sample(base_sample(ProbeType::Tcp, 1), 40),
        ok_sample(https(1), 300),
        ok_sample(https(2), 310),
        ok_sample(https(3), 320),
    ];
    let run = make_run(
        RouteMode::ManualLocalProxy,
        RunStatus::Completed,
        3,
        samples,
    );
    let out = diagnose(&[run]);
    assert!(find_rule(&out, "R01").is_some(), "DNS 基线失败应如实报告");
    assert!(
        find_rule(&out, "R11").is_none(),
        "HTTPS 全成功不构成基线分歧"
    );
    assert!(find_rule(&out, "R09").is_none(), "代理可用不应报不可用");
}

#[test]
fn tcp_failure_but_proxy_https_success() {
    let samples = vec![
        ok_sample(base_sample(ProbeType::Dns, 1), 12),
        transport(base_sample(ProbeType::Tcp, 1), ErrorKind::TcpFailure),
        ok_sample(https(1), 300),
        ok_sample(https(2), 300),
        ok_sample(https(3), 300),
    ];
    let run = make_run(
        RouteMode::ManualLocalProxy,
        RunStatus::Completed,
        3,
        samples,
    );
    let out = diagnose(&[run]);
    assert!(find_rule(&out, "R02").is_some());
    assert!(find_rule(&out, "R11").is_none());
}

#[test]
fn multiple_rules_fire_together_in_deterministic_order() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            status_sample(https(1), 401),
            transport(https(2), ErrorKind::Timeout),
            ok_sample(https(3), 200),
        ],
    );
    let out = diagnose(&[run]);
    let ids = rule_ids(&out);
    assert!(
        ids.contains(&"R04".to_string()) && ids.contains(&"R05".to_string()),
        "{ids:?}"
    );
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "输出必须按 rule_id 确定性排序");
}

#[test]
fn all_samples_normal_produces_no_diagnoses() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            ok_sample(https(2), 400),
            ok_sample(https(3), 400),
        ],
    );
    let out = diagnose(&[run]);
    assert!(out.is_empty(), "全部正常不应产生诊断：{:?}", rule_ids(&out));
}

#[test]
fn all_samples_failed_transport_produces_matching_rules() {
    let run = run_with_https(
        RouteMode::NoAppProxy,
        vec![transport(https(1), ErrorKind::TlsFailure); 3],
    );
    let out = diagnose(&[run]);
    assert!(find_rule(&out, "R03").is_some());
    assert!(find_rule(&out, "R11").is_some());
}

#[test]
fn deterministic_output_across_calls() {
    let good = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            ok_sample(https(2), 400),
            ok_sample(https(3), 400),
        ],
    );
    let bad = run_with_https(
        RouteMode::ManualLocalProxy,
        vec![transport(https(1), ErrorKind::ProxyUnavailable); 3],
    );
    let runs = vec![good, bad];
    let first = diagnose(&runs);
    let second = diagnose(&runs);
    assert_eq!(first.len(), second.len());
    for (left, right) in first.iter().zip(second.iter()) {
        assert_eq!(left.rule_id, right.rule_id);
        assert_eq!(left.title, right.title);
        assert_eq!(left.observed_facts, right.observed_facts);
        assert_eq!(left.possible_causes, right.possible_causes);
        assert_eq!(
            format!("{left:?}"),
            format!("{right:?}"),
            "诊断必须完全一致"
        );
    }
}

#[test]
fn empty_input_produces_empty_output() {
    assert!(diagnose(&[]).is_empty());
}

#[test]
fn evidence_and_causes_are_structurally_separated() {
    let scenarios: Vec<DiagnosticRun> = vec![
        run_with_https(
            RouteMode::NoAppProxy,
            vec![transport(https(1), ErrorKind::TlsFailure); 3],
        ),
        run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 429); 3]),
        run_with_https(
            RouteMode::ManualLocalProxy,
            vec![transport(https(1), ErrorKind::ProxyUnavailable); 3],
        ),
    ];
    for run in &scenarios {
        for diagnosis in diagnose(std::slice::from_ref(run)) {
            assert!(
                !diagnosis.observed_facts.is_empty(),
                "{} 缺事实",
                diagnosis.rule_id
            );
            assert!(
                !diagnosis.possible_causes.is_empty() && diagnosis.possible_causes.len() <= 3,
                "{} 原因数量越界",
                diagnosis.rule_id
            );
            assert!(
                !diagnosis.limitations.is_empty(),
                "{} 缺局限",
                diagnosis.rule_id
            );
        }
    }
}

#[test]
fn no_unverified_assertions_in_titles_or_causes() {
    // 禁止事项扫描：标题与可能原因中不得出现未经证实的断言
    // （limitations 中的"不能推断 XXX"句式不在扫描范围）。
    let forbidden = [
        "信誉",
        "封锁",
        "绕过",
        "账号",
        "必然更快",
        "一定宕机",
        "登录成功",
        "可正常登录",
    ];
    let scenarios: Vec<DiagnosticRun> = vec![
        run_with_https(
            RouteMode::NoAppProxy,
            vec![transport(https(1), ErrorKind::DnsFailure); 3],
        ),
        run_with_https(
            RouteMode::NoAppProxy,
            vec![transport(https(1), ErrorKind::TcpFailure); 3],
        ),
        run_with_https(
            RouteMode::NoAppProxy,
            vec![transport(https(1), ErrorKind::TlsFailure); 3],
        ),
        run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 401); 3]),
        run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 403); 3]),
        run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 407); 3]),
        run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 429); 3]),
        run_with_https(RouteMode::NoAppProxy, vec![status_sample(https(1), 503); 3]),
        run_with_https(
            RouteMode::ManualLocalProxy,
            vec![transport(https(1), ErrorKind::ProxyUnavailable); 3],
        ),
        run_with_https(
            RouteMode::NoAppProxy,
            vec![
                ok_sample(https(1), 400),
                ok_sample(https(2), 400),
                ok_sample(https(3), 400),
            ],
        ),
    ];
    let good = run_with_https(
        RouteMode::NoAppProxy,
        vec![
            ok_sample(https(1), 400),
            ok_sample(https(2), 400),
            ok_sample(https(3), 400),
        ],
    );
    let bad = run_with_https(
        RouteMode::ManualLocalProxy,
        vec![transport(https(1), ErrorKind::ProxyUnavailable); 3],
    );
    // R10 跨运行场景
    let all = [scenarios, vec![good, bad]].concat();
    for run in &all {
        for diagnosis in diagnose(std::slice::from_ref(run)) {
            for cause in &diagnosis.possible_causes {
                for word in forbidden {
                    assert!(
                        !cause.contains(word),
                        "{} 的原因包含未经证实断言 {word:?}：{cause}",
                        diagnosis.rule_id
                    );
                }
            }
            for word in forbidden {
                assert!(
                    !diagnosis.title.contains(word),
                    "{} 的标题包含断言 {word:?}",
                    diagnosis.rule_id
                );
            }
        }
    }
    // R10 自身也扫描
    for diagnosis in diagnose(&all[all.len() - 2..]) {
        for cause in &diagnosis.possible_causes {
            for word in forbidden {
                assert!(!cause.contains(word), "R10 原因含断言 {word:?}：{cause}");
            }
        }
    }
}

#[test]
fn confidence_serializes_as_evidence_level_not_probability() {
    assert_eq!(
        serde_json::to_string(&Confidence::EvidenceSufficient).unwrap(),
        "\"evidence_sufficient\""
    );
    assert_eq!(
        serde_json::to_string(&Confidence::Possible).unwrap(),
        "\"possible\""
    );
    assert_eq!(
        serde_json::to_string(&Confidence::Insufficient).unwrap(),
        "\"insufficient\""
    );
}
