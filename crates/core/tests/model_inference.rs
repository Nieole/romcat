//! **模型推断兜底那一层的接缝**：磁盘上摆一份主库、手边一个**假服务器**，跑一遍识别，
//! 看这三条硬约束有没有真的接在扫描、识别与**待确认队列**之间。
//!
//! | 硬约束 | 这里怎么证 |
//! |---|---|
//! | 只处理**前面各层全部落空**的变体 | 精确命中的那一份与文件名那一层认出来的那一份，**一个字都没进请求正文** |
//! | **批量打包**提问 | 四个变体一个请求，正文里编号从 1 数到 4——`CannedFetcher::posted` 拿得到正文 |
//! | 输出**永不自动通过** | 候选 `accepted` 全假、置信度低置信、**依据**第一句就说它是模型推断的、而且**在队列里** |
//!
//! ## 为什么是假服务器
//!
//! 真实凭据这一趟拿不到（票 14 同一条纪律：不申请、不拿用户的账号做实验）。
//! 假服务器与真服务**共用同一个 [`Fetcher`] 接缝、同一道 `dat::guard` 闸门、
//! 同一道 `require_ok` 状态码处置**——差别只在字节从哪儿来。

use std::fs;
use std::path::Path;

use romcat_core::catalog::identify::State;
use romcat_core::catalog::{Candidate, Catalog, Confidence};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::dat::{CannedFetcher, Fetcher};
use romcat_core::filename::Rules;
use romcat_core::fs::RealFs;
use romcat_core::identify::model::{self, Credentials, Guessing, Inference, Limits, Pricing};
use romcat_core::identify::{self, Options, fuzzy};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::triage::{self, Filter};
use romcat_core::verdict;
use romcat_core::zh;

fn 卡带(fill: u8) -> Vec<u8> {
    vec![fill; 4_096]
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

const 精确命中的: &str = "gba/Known Game (Japan).zip";
const 名字撞得上的: &str = "gba/合金弹头7[某汉化组].zip";
const 残渣一: &str = "gba/033.動作：掃地雷.zip";
const 残渣二: &str = "gba/ACGHH-0113-TP.zip";
const 残渣三: &str = "gba/my_theme0.zip";
const 补丁: &str = "gba/流星洛克人 3 汉化补丁.zip";

fn 建现场() -> 现场 {
    let dir = temp_dir("model");
    let root = dir.path();

    // 一、**精确哈希命中**：这一层连碰都不该碰它。
    写(
        &root.join(精确命中的),
        &zip_container(&[ZipEntrySpec::stored("known.gba", 卡带(0xA1))]),
    );
    // 二、**文件名那一层认得出来**：已经有候选可看了，不重复花钱。
    写(
        &root.join(名字撞得上的),
        &zip_container(&[ZipEntrySpec::stored("m7.gba", 卡带(0xB2))]),
    );
    // 三、**残渣**：前面每一层都落空，名字一个比一个怪。真库里的形状。
    写(
        &root.join(残渣一),
        &zip_container(&[ZipEntrySpec::stored("033.gba", 卡带(0xC3))]),
    );
    写(
        &root.join(残渣二),
        &zip_container(&[ZipEntrySpec::stored("acghh.gba", 卡带(0xC4))]),
    );
    写(
        &root.join(残渣三),
        &zip_container(&[ZipEntrySpec::stored("theme.gba", 卡带(0xC5))]),
    );
    // 四、**补丁**：识别把它判成「跳过」。词表原话：拿补丁去撞 DAT 必然落空，
    //     会一路掉到模型推断白烧一遍——**这一层一个字都不该问它**。
    写(
        &root.join(补丁),
        &zip_container(&[ZipEntrySpec::stored("rockman.ips", 卡带(0xC6))]),
    );

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(root);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        repo: 建_dat(),
    }
}

fn 建_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("能开 DAT 库");
    let bytes = 卡带(0xA1);
    let mut writer = repo
        .begin(&Unit {
            source: "No-Intro".to_string(),
            name: "Nintendo - Game Boy Advance".to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: "Nintendo - Game Boy Advance".to_string(),
                platform: "GBA".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[GameRecord {
                name: "Known Game (Japan)".to_string(),
                roms: vec![RomRecord {
                    name: "known.gba".to_string(),
                    size: Some(bytes.len() as u64),
                    crc32: Some(crc32(&bytes)),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写得进");
    writer.commit().expect("提交");
    repo
}

fn 建索引() -> zh::Index {
    zh::Index::build(
        vec![zh::Entry {
            id: 4,
            name: "メタルスラッグ7".to_string(),
            name_cn: "合金弹头7".to_string(),
            aliases: Vec::new(),
            year: Some(2008),
            platforms: vec!["GBA".to_string()],
            platform_text: "GBA".to_string(),
            ..zh::Entry::default()
        }],
        "dump-2026-09-01".to_string(),
    )
}

/// 假服务器答的那一份。**编号是按批次的 1..=n**，所以同一份答复对每一批都对得上号。
fn 一份答复(n: usize) -> Vec<u8> {
    let rows: Vec<serde_json::Value> = (1..=n)
        .map(|id| {
            serde_json::json!({
                "编号": id,
                "猜测": [
                    { "标题": format!("模型猜的第{id}个"), "平台": "GBA", "依据": "名字里那串编号看着像整理者加的前缀" },
                    { "标题": format!("模型猜的第{id}个备选"), "平台": "GBA", "依据": "同系列" }
                ]
            })
        })
        .collect();
    let text = serde_json::json!({ "答案": rows }).to_string();
    serde_json::to_vec(&serde_json::json!({
        "model": "claude-opus-5",
        "content": [{ "type": "text", "text": text }],
        "usage": { "input_tokens": 3_000, "output_tokens": 600 }
    }))
    .expect("造得出")
}

fn 宽松上限() -> Limits {
    Limits {
        interval: std::time::Duration::from_millis(0),
        backoff: std::time::Duration::from_millis(0),
        ..Limits::default()
    }
}

/// 跑一趟识别。`fetcher` 是 `None` 时这一层**只用缓存、一个请求都不发**。
fn 跑一趟(
    现场: &mut 现场, fetcher: Option<&dyn Fetcher>, limits: Limits
) -> identify::Outcome {
    let rules = Rules::builtin();
    let index = 建索引();
    let naming = fuzzy::Naming {
        rules: &rules,
        index: Some(&index),
        tuning: zh::Tuning::default(),
    };
    let answers = model::Answers::build(现场.catalog.model_answers().expect("读得到"));
    let price = Pricing::builtin()
        .price(model::DEFAULT_MODEL)
        .expect("默认模型有价");
    let cancel = CancelToken::new();
    let net = fetcher.map(|fetcher| {
        Inference::new(
            fetcher,
            limits.clone(),
            price,
            Credentials::api_key("测试用的假凭据"),
            &cancel,
        )
    });
    let 摆出来 = std::cell::RefCell::new(Vec::new());
    let announce = |plan: &model::Plan| 摆出来.borrow_mut().push(plan.clone());
    let guessing = Guessing {
        answers: &answers,
        net: net.as_ref(),
        announce: Some(&announce),
        // 没有网络句柄的那一趟走的正是 `--model-plan` 那条路：计划算得出，一个请求都不发。
        planning: net.is_none(),
        model: model::DEFAULT_MODEL.to_string(),
        price,
        checked: "2026-09-02".to_string(),
        limits,
    };
    let options = Options::new(现场.dir.path());
    let outcome = identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::empty(),
            naming: &naming,
            guessing: &guessing,
            titledb: None,
        },
        &options,
        &cancel,
        &mut |_| {},
    )
    .expect("识别不该失败");
    // **计划必须在第一个请求发出去之前就摆出来**——它是「花费可预估」这条验收的落点。
    // 全部命中缓存的那一趟没有计划可摆：那时一分钱都不会花，摆一份「花 0 美元」的
    // 计划只是噪音。
    let 该摆几次 = usize::from(outcome.model.residue > outcome.model.from_cache);
    assert_eq!(摆出来.borrow().len(), 该摆几次, "计划摆出来的次数不对");
    outcome
}

fn 候选(现场: &现场, key: &str) -> Vec<Candidate> {
    现场.catalog.candidates_of(key).expect("读得到候选")
}

/// 每个请求正文里那一段**给模型看的提示词**。
///
/// 从 JSON 里取而不是拿整份正文当字符串搜：正文里 `\n` 是转义序列，
/// 按行拆是拆不开的，而「问了谁」这件事只有按行才看得出来。
fn 正文(fetcher: &CannedFetcher) -> Vec<String> {
    fetcher
        .posted()
        .iter()
        .map(|body| {
            let value: serde_json::Value = serde_json::from_slice(body).expect("正文该是 JSON");
            value["messages"][0]["content"]
                .as_str()
                .expect("提示词该在这儿")
                .to_string()
        })
        .collect()
}

#[test]
fn 只问前面各层全部落空的变体() {
    let mut 现场 = 建现场();
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let outcome = 跑一趟(&mut 现场, Some(&fetcher), 宽松上限());

    // 残渣正好三个：精确命中的、名字撞得上的、补丁，三个都不在里面。
    assert_eq!(outcome.model.residue, 3, "{:?}", outcome.model);
    let 正文 = 正文(&fetcher);
    assert_eq!(正文.len(), 1, "三条打成一个请求");
    let one = &正文[0];
    for 残渣 in [残渣一, 残渣二, 残渣三] {
        let name = 残渣.rsplit('/').next().expect("有文件名");
        assert!(one.contains(name), "残渣 {name} 该在请求里：{one}");
    }
    // **已经有候选的不重复花钱**，**跳过的一个字都不问**。判据是「有没有被当成
    // 一条问题问出去」，不是「名字有没有出现过」——它们照样会作为**同目录的别的文件**
    // 出现在上下文里，而那正是票 12 点名要给的三样之一。
    let 问了谁: Vec<&str> = one
        .lines()
        .filter_map(|line| line.strip_prefix("- 名字（变体自己的名字）："))
        .collect();
    assert_eq!(问了谁.len(), 3, "{问了谁:?}");
    for 不该问 in [
        "Known Game (Japan).zip",
        "合金弹头7[某汉化组].zip",
        "流星洛克人 3 汉化补丁.zip",
    ] {
        assert!(!问了谁.contains(&不该问), "{不该问} 不该被当成问题问出去");
    }
    // 但它们**该**出现在上下文里。
    assert!(one.contains("同目录还有"));
}

#[test]
fn 一个请求装一整批而不是一条一发() {
    let mut 现场 = 建现场();
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let outcome = 跑一趟(&mut 现场, Some(&fetcher), 宽松上限());

    // 三个变体、一个请求。**逐条问会是三个**。
    assert_eq!(fetcher.asked().len(), 1);
    assert_eq!(outcome.model.usage.requests, 1);
    assert_eq!(outcome.model.batch_size, model::DEFAULT_BATCH as u64);
    let one = &正文(&fetcher)[0];
    assert!(one.contains("### 第 1 条"));
    assert!(one.contains("### 第 3 条"));
    assert!(one.contains("一共 3 条，编号 1 到 3"));
}

#[test]
fn 打包大小说了算_三条分两批() {
    let mut 现场 = 建现场();
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let limits = Limits {
        batch: 2,
        ..宽松上限()
    };
    跑一趟(&mut 现场, Some(&fetcher), limits);
    assert_eq!(fetcher.asked().len(), 2, "三条按两条一批打成两个请求");
}

#[test]
fn 这一层的候选永不自动通过而且进了待确认队列() {
    let mut 现场 = 建现场();
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    跑一趟(&mut 现场, Some(&fetcher), 宽松上限());

    let 候选 = 候选(&现场, 残渣一);
    assert_eq!(候选.len(), 2, "{候选:?}");
    for one in &候选 {
        // ADR-0002：模型的输出永远进队列、永不自动通过。
        assert!(!one.accepted);
        assert_eq!(one.confidence, Confidence::Low);
        assert_eq!(one.source, model::SOURCE);
        // **依据明确说它是模型推断的**（工单原话：依据要标注这一层的出处）。
        assert!(
            one.evidence.starts_with("**这一条是模型推断的**"),
            "{}",
            one.evidence
        );
        assert!(one.evidence.contains("claude-opus-5"));
    }

    // 真的在**待确认队列**里。
    let queue = triage::survey(&现场.catalog, &verdict::Index::empty(), &Filter::default())
        .expect("队列列得出");
    assert!(
        queue.items.iter().any(|row| row.variant.key == 残渣一),
        "模型推断的候选必须进队列"
    );
    // 精确命中的那一份**不在**队列里（它自动通过了）。
    assert!(!queue.items.iter().any(|row| row.variant.key == 精确命中的));
}

#[test]
fn 结论的状态与那句_为什么没定下来_一个字都没被猜测覆盖() {
    // 与文件名那一层**故意不同**：「无判据」那一列的理由是真事实
    // （rar 穿不透、元数据读不到），让一句猜测覆盖掉是净损失。
    let mut 现场 = 建现场();
    // 先跑一趟不问的，把「这一层没插手时结论长什么样」记下来。
    跑一趟(&mut 现场, None, 宽松上限());
    let 之前 = 现场
        .catalog
        .identification_of(残渣一)
        .expect("读得到")
        .expect("有结论");
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    跑一趟(&mut 现场, Some(&fetcher), 宽松上限());
    let 之后 = 现场
        .catalog
        .identification_of(残渣一)
        .expect("读得到")
        .expect("有结论");
    assert_eq!(之后.0, State::Unmatched, "状态不许被猜测抬成命中");
    assert_eq!(之前, 之后, "状态与「为什么没定下来」那两列一个字都不许变");
    assert!(!候选(&现场, 残渣一).is_empty(), "但候选照样落下来了");
}

#[test]
fn 问过的答案落库_重跑一趟一个请求都不发() {
    let mut 现场 = 建现场();
    let 第一趟 = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let out1 = 跑一趟(&mut 现场, Some(&第一趟), 宽松上限());
    assert_eq!(out1.model.from_cache, 0);
    assert_eq!(第一趟.asked().len(), 1);

    // 第二趟：同一个模型、同一套参数——**指纹一模一样，于是白拿**。
    let 第二趟 = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let out2 = 跑一趟(&mut 现场, Some(&第二趟), 宽松上限());
    assert_eq!(
        第二趟.asked().len(),
        0,
        "问过的不许再问一遍——那是真的再付一次钱"
    );
    assert_eq!(out2.model.from_cache, 3);
    assert_eq!(out2.model.residue, 3);
    // 候选照样在库里：`clear_identifications` 清掉的是候选，答案缓存活着。
    assert_eq!(候选(&现场, 残渣一).len(), 2);
}

#[test]
fn 换一档力度会重问() {
    let mut 现场 = 建现场();
    let 第一趟 = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    跑一趟(&mut 现场, Some(&第一趟), 宽松上限());

    let 第二趟 = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let limits = Limits {
        effort: "max".to_string(),
        ..宽松上限()
    };
    跑一趟(&mut 现场, Some(&第二趟), limits);
    assert_eq!(第二趟.asked().len(), 1, "力度进提问指纹，换一档该重问");
}

#[test]
fn 只用缓存那一趟一个请求都不发_计划照样算得出() {
    let mut 现场 = 建现场();
    // 没有网络句柄（`--model-plan` 与「这一趟不开这一层」都是这个形状）。
    let outcome = 跑一趟(&mut 现场, None, 宽松上限());
    assert_eq!(outcome.model.usage.requests, 0);
    assert!(候选(&现场, 残渣一).is_empty());
    // 但**计划算得出来**：问多少个、打成几个请求、花费的下界与上界。
    let plan = outcome.model.plan.expect("计划该算得出");
    assert_eq!(plan.variants, 3);
    assert_eq!(plan.batches, 1);
    assert!(plan.floor_micros > 0);
    assert!(plan.ceiling_micros > plan.floor_micros);
    assert_eq!(plan.model, model::DEFAULT_MODEL);
    assert_eq!(plan.priced_at, "2026-09-02");
}

#[test]
fn 请求上限到了就停_已经问到的照样落库() {
    let mut 现场 = 建现场();
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let limits = Limits {
        batch: 1,
        budget: 2,
        ..宽松上限()
    };
    let outcome = 跑一趟(&mut 现场, Some(&fetcher), limits);
    assert_eq!(fetcher.asked().len(), 2, "上限是 2 就只发 2 个");
    assert_eq!(outcome.model.halt, Some(model::HaltKind::Self_));
    assert!(
        outcome
            .model
            .halted
            .as_deref()
            .unwrap_or_default()
            .contains("请求上限")
    );
    // **停下来不是错误**：已经问到的两条照样落库了。
    assert_eq!(outcome.model.asked, 2);
    assert_eq!(现场.catalog.model_answers().expect("读得到").len(), 2);
}

#[test]
fn 凭据不对就停_一个请求都不再发() {
    let mut 现场 = 建现场();
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 401, b"nope".to_vec());
    let limits = Limits {
        batch: 1,
        ..宽松上限()
    };
    let outcome = 跑一趟(&mut 现场, Some(&fetcher), limits);
    assert_eq!(fetcher.asked().len(), 1, "**绝不换一套再试**");
    assert_eq!(outcome.model.halt, Some(model::HaltKind::Refused));
    assert!(候选(&现场, 残渣一).is_empty());
}

#[test]
fn 一次提问的账落进库里() {
    let mut 现场 = 建现场();
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
    let outcome = 跑一趟(&mut 现场, Some(&fetcher), 宽松上限());
    // 3000 输入 × 500 分 + 600 输出 × 2500 分，除以 100 = 30,000 微美元 = 0.03 美元。
    assert_eq!(outcome.model.usage.cost_micros, 30_000);
    assert_eq!(model::dollars(outcome.model.usage.cost_micros), "0.03 美元");
    let (calls, micros) = 现场.catalog.model_spend().expect("读得到总账");
    assert_eq!(calls, 1);
    assert_eq!(micros, 30_000);
}

#[test]
fn 模型说不出的那些一条候选都不产出() {
    let mut 现场 = 建现场();
    let 空答复 = serde_json::to_vec(&serde_json::json!({
        "model": "claude-opus-5",
        "content": [{ "type": "text", "text": "{\"答案\":[{\"编号\":1,\"猜测\":[]},\
            {\"编号\":2,\"猜测\":[]},{\"编号\":3,\"猜测\":[]}]}" }],
        "usage": { "input_tokens": 100, "output_tokens": 10 }
    }))
    .expect("造得出");
    let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 空答复);
    let outcome = 跑一趟(&mut 现场, Some(&fetcher), 宽松上限());
    assert_eq!(outcome.model.speechless, 3);
    assert_eq!(outcome.model.candidates, 0);
    assert!(候选(&现场, 残渣一).is_empty());
    // **说不出也是一条答复**：它落了库，下一趟不会为同一个问题再付一次钱。
    assert_eq!(现场.catalog.model_answers().expect("读得到").len(), 3);
}
