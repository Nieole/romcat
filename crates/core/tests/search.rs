//! **搜索框与匹配质量排序**（票 `gui-redesign/05`）。
//!
//! 这一层要钉住的是四件事：
//!
//! 1. **匹配得好的排前面**：以搜索词开头 > 含有 > 别名以它开头 > 别名含有 > 简介提到。
//!    权重内置，用户配不了——所以它只能在这儿验，界面上没有旋钮可拨。
//! 2. **中文与拉丁文都搜得动**。中文那一侧几乎整批落在**标题集合**上（作品名来自 DAT，
//!    而 DAT 里没有中文），所以别名那一档不分「开头/含有」的话，中文搜索根本排不出
//!    次序来。**繁简不折**是现状（挂账 D121），这里照现状钉着。
//! 3. **搜索与筛选叠加**：先筛后搜，结果既满足条件、又按质量排。
//! 4. **排序不进子库的规则**：存成子库时只带条件不带顺序——搜索框当场挡住，
//!    而排的哪一列、正着倒着，`to_rule` 连读都不读。

use romcat_core::catalog::browse::{SearchHit, Unruly, WorkAnchor, WorkOrder, WorkQuery, WorkRow};
use romcat_core::catalog::identify::{Identification, Provenance};
use romcat_core::catalog::title::TitleRow;
use romcat_core::catalog::{Catalog, Confidence, State};
use romcat_core::platform::Manifest;
use romcat_core::scrape::{AnchorKind, Field};
use romcat_core::shape::{Role, Variant};
use romcat_core::sublibrary::Rule;
use romcat_core::title::{Language, TitleKind};

/// 中文那个搜索词。真库里最要紧的那批内容全靠它这条路找得到。
const 中文词: &str = "弹头";

/// 拉丁字母那个搜索词。**故意用小写**：库里那几个名字是大写开头的，
/// 大小写不敏感这一条顺带钉住。
const 拉丁词: &str = "slug";

/// 这份 fixture 里的作品：名字、标题集合里另一条叫法、简介。
///
/// 五档命中各安排一个，外加一条**一档都不沾**的（搜出来不该有它）与一条**繁体**的
/// （简体的搜索词碰不到它——那是现状，不是目标）。
const 作品: &[(&str, Option<&str>, Option<&str>)] = &[
    // 名字以「弹头」开头。
    ("弹头对策室", None, None),
    // 名字含有「弹头」。
    ("合金弹头 2", None, None),
    // 名字里一个中文都没有，中文名在**标题集合**里，而且以「弹头」开头。
    ("Metal Slug", Some("弹头传奇"), None),
    // 同上，只是那条中文名**含有**「弹头」而不是以它开头。
    ("Metal Slug X", Some("合金弹头 X"), None),
    // 只有**简介**里提到「弹头」。
    ("Contra", None, Some("一款横版射击，弹头满天飞。")),
    // 拉丁词那一趟的「以它开头」。
    ("Slug Fest", None, None),
    // **一档都不沾**：三条命中路一条都撞不上。
    ("Gradius", Some("グラディウス"), Some("宇宙战机。")),
    // **繁体**：简体的「弹头」搜不到它，繁体的「彈頭」才行（挂账 D121）。
    ("合金彈頭 3", None, None),
];

/// 那个**还没认出作品**的变体叫什么。它没有标题集合，屏上那个名字就是它自己的键——
/// 「按路径找那条路走的是搜索框」（挂单 Q74）在这一条上看得见。
const 散键: &str = "主库/未知/弹头 未认出.zip";

/// 另一个**还没认出作品**的变体：名字里一个搜索词都没有，简介写在**它自己**头上。
/// 没认出作品的那些只有变体锚点，所以这一条走的就是它们唯一的那条路。
const 散键简介: &str = "主库/未知/无名转储.zip";

/// 靠**简介**命中的那个作品。**它底下挂着好几个变体**：搜索是「找这一行」，
/// 不该顺手把这一行有几个变体改掉——这一条只有在多变体的行上才验得出来。
const 简介作品: &str = "Contra";

/// [`简介作品`] 底下挂几个变体。
const 简介作品变体数: u64 = 3;

/// 只写在**变体锚点**上的那条简介里有这个词。
///
/// 主列表这一行**不认它**：那一行「元数据齐不齐」里的简介是按**作品锚点**判的
/// （`fill_missing`），年份那一列也是——搜索挑另一条口径的话，屏上一行写着「缺简介」，
/// 搜索却说它「简介里提到它」。而**规则语言**那一侧是「两个锚点合起来看」，
/// 找得到它，两条口径的分野由 `变体锚点上的简介不算主列表这一行的` 钉着。
const 变体简介词: &str = "潜水艇";

fn 变体(key: &str) -> Variant {
    Variant {
        key: key.to_string(),
        platform: Some("GB".to_string()),
        rule: "一文件一变体".into(),
        main_key: key.to_string(),
        manual: false,
        files: 1,
        bytes: 1_000,
        unreadable_files: 0,
        members: vec![(key.to_string(), Role::Main)],
    }
}

/// 作品 `at` 底下第 `n` 个变体的键。**故意不带中文也不带 `slug`**：不然名字那一条
/// 命中路与键那一条会混在一起，测出来的次序说不清是哪一条给的。
fn 键(at: usize, n: u64) -> String {
    format!("主库/GB/w{at:02}-{n}.zip")
}

/// 作品 `at` 底下挂几个变体。
fn 变体数(at: usize) -> u64 {
    if 作品[at].0 == 简介作品 {
        简介作品变体数
    } else {
        1
    }
}

/// 这份 fixture 一共出多少行：一个作品一行，加上两个**还没认出作品**的各自一行。
fn 总行数() -> u64 {
    作品.len() as u64 + 2
}

fn 建库() -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut variants: Vec<Variant> = Vec::new();
    for at in 0..作品.len() {
        variants.extend((0..变体数(at)).map(|n| 变体(&键(at, n))));
    }
    variants.push(变体(散键));
    variants.push(变体(散键简介));
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进去");

    let mut records = Vec::new();
    let mut titles = Vec::new();
    for (at, (name, alias, description)) in 作品.iter().enumerate() {
        let work = catalog
            .add_work(name, Provenance::Identified)
            .expect("建得出作品");
        records.extend((0..变体数(at)).map(|n| Identification {
            variant_key: 键(at, n),
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work),
            release_id: None,
            candidates: Vec::new(),
        }));
        if let Some(alias) = alias {
            titles.push(TitleRow {
                work: (*name).to_string(),
                value: (*alias).to_string(),
                language: Language::Chinese,
                kind: TitleKind::Alias,
                source: "中文离线源".to_string(),
                region: None,
                variant_key: None,
                confidence: Confidence::High,
                seam: None,
                evidence: "fixture 里钉死的别名".to_string(),
                seen: 1,
            });
        }
        if let Some(description) = description {
            catalog
                .put_verdict_value(
                    AnchorKind::Work,
                    name,
                    Field::Description,
                    description,
                    "fixture",
                )
                .expect("写得进简介");
        }
        // **年份**：这一列在主列表那条查询里是单独一张 join，而搜索那一档的参数排在它
        // 前面（`SELECT` 在 `FROM` 之前）。两处参数绑串了的话年份就会读成别的东西
        // ——所以这份 fixture 必须有年份可读，见 `搜索着的时候年份那一列照样读得对`。
        catalog
            .put_verdict_value(
                AnchorKind::Work,
                name,
                Field::Year,
                &format!("{}", 1990 + at),
                "fixture",
            )
            .expect("写得进年份");
    }
    // 一条**只写在变体锚点上**的简介，挂在一个作品底下的某个变体上。
    // 主列表这一行不认它，规则语言那一侧认——两条口径的分野由测试钉着。
    catalog
        .put_verdict_value(
            AnchorKind::Variant,
            &键(0, 0),
            Field::Description,
            &format!("这一段只写在变体头上，里面有{变体简介词}。"),
            "fixture",
        )
        .expect("写得进简介");
    // **没认出作品的那些只有变体锚点**，所以它们的简介只可能写在这儿。
    catalog
        .put_verdict_value(
            AnchorKind::Variant,
            散键简介,
            Field::Description,
            &format!("一份没认出来的转储，简介里提到{中文词}。"),
            "fixture",
        )
        .expect("写得进简介");
    // **散落那两个变体一行结论都不写**：那是「还没识别」，它们照样得搜得出来。
    catalog
        .write_identifications(&records)
        .expect("写得进识别结论");
    catalog.put_titles(&titles).expect("写得进标题集合");
    catalog
}

/// 搜一趟，把 `(名字, 命中在哪儿)` 按屏上的次序收成一串。
fn 搜(catalog: &Catalog, query: &WorkQuery) -> Vec<(String, SearchHit)> {
    catalog
        .work_page(query, 0, 64)
        .expect("取得出一页")
        .into_iter()
        .map(|row: WorkRow| {
            let hit = row.hit.expect("搜索着的时候每一行都该说得出命中在哪儿");
            (row.name, hit)
        })
        .collect()
}

fn 搜词(catalog: &Catalog, needle: &str) -> Vec<(String, SearchHit)> {
    搜(
        catalog,
        &WorkQuery {
            search: needle.to_string(),
            ..WorkQuery::default()
        },
    )
}

#[test]
fn 以搜索词开头的排在含有那个词的前面() {
    let catalog = 建库();

    // 中文那一趟。「弹头对策室」以它开头，「合金弹头 2」只是含有它。
    let 中文 = 搜词(&catalog, 中文词);
    let 名字: Vec<&str> = 中文.iter().map(|(name, _)| name.as_str()).collect();
    let 开头 = 名字.iter().position(|n| *n == "弹头对策室").expect("搜得到");
    let 含有 = 名字.iter().position(|n| *n == "合金弹头 2").expect("搜得到");
    assert!(开头 < 含有, "以搜索词开头的没排在含有它的前面：{名字:?}");

    // 拉丁那一趟，而且**大小写不敏感**：打的是小写 `slug`，库里是 `Slug`。
    let 拉丁 = 搜词(&catalog, 拉丁词);
    assert_eq!(
        拉丁.first().map(|(name, hit)| (name.as_str(), *hit)),
        Some(("Slug Fest", SearchHit::TitleStart)),
        "以搜索词开头的没排最前：{拉丁:?}",
    );
    assert!(
        拉丁
            .iter()
            .any(|(name, hit)| name == "Metal Slug" && *hit == SearchHit::Title),
        "含有那一档没认出来：{拉丁:?}",
    );

    // **一档都不沾的搜不出来**：三条命中路一条都撞不上。
    assert!(
        !中文.iter().any(|(name, _)| name == "Gradius"),
        "一条都没命中的行也进了结果：{中文:?}",
    );
}

#[test]
fn 别名命中与简介命中排在标题命中之后而且各自的次序确定() {
    let catalog = 建库();
    let 结果 = 搜词(&catalog, 中文词);

    // **五档的次序就是屏上的次序**：`hit` 一路不减。
    let mut 上一档 = SearchHit::TitleStart;
    for (name, hit) in &结果 {
        assert!(*hit >= 上一档, "「{name}」排到了比它更好的一档前面：{结果:?}");
        上一档 = *hit;
    }

    // 每一档各是谁，一条条点名——「各自的次序确定」说的就是这个。
    let 档 = |name: &str| {
        结果
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("「{name}」没搜出来：{结果:?}"))
            .1
    };
    assert_eq!(档("弹头对策室"), SearchHit::TitleStart);
    assert_eq!(档("合金弹头 2"), SearchHit::Title);
    // **别名那一档也分开头与含有**：中文名整批落在这儿，不分开就排不出次序（挂单 Q104）。
    assert_eq!(档("Metal Slug"), SearchHit::AliasStart);
    assert_eq!(档("Metal Slug X"), SearchHit::Alias);
    assert_eq!(档("Contra"), SearchHit::Description);

    // **没认出作品的那一行按它自己的键搜**（挂单 Q74）：它没有标题集合，
    // 屏上那个名字就是那个键，于是它落在标题那一族而不是别名那一族。
    // 是**含有**那一档而不是「以它开头」——键的头上是根名（`主库/未知/…`），
    // 按路径找永远从中间撞上，这是键那条路的形状，不是漏了一档。
    assert_eq!(档(散键), SearchHit::Title);
}

#[test]
fn 靠简介命中的那一行变体数一个都不少() {
    // **搜索是「找这一行」，不是「筛这一行底下的变体」。** 收窄落在 `WHERE` 上、
    // 逐个变体行判，所以那三条命中路必须**组内恒定**；简介那一条若按变体锚点比
    // （`catalog::filter` 的口径），一部挂着 3 个变体的作品只要有一个变体身上写着简介，
    // 分完组之后这一行就只剩那 1 个——屏上「变体数」写 1 而不是 3，容量与平台集合
    // 跟着缩水，随后「全选 → 批量刮削」也只作用到那一个。
    let catalog = 建库();
    let rows = catalog
        .work_page(
            &WorkQuery {
                search: 中文词.to_string(),
                ..WorkQuery::default()
            },
            0,
            64,
        )
        .expect("取得出一页");
    let 那一行 = rows
        .iter()
        .find(|row| row.name == 简介作品)
        .unwrap_or_else(|| panic!("靠简介该搜得到「{简介作品}」"));
    assert_eq!(那一行.hit, Some(SearchHit::Description));
    assert_eq!(
        那一行.variants, 简介作品变体数,
        "靠简介命中把这一行的变体数缩水了",
    );

    // **没认出作品的那一行**只有变体锚点，简介写在它自己头上——那条路照样通。
    let 散的 = rows
        .iter()
        .find(|row| row.name == 散键简介)
        .expect("没认出作品的那一行按它自己的变体锚点也该搜得到");
    assert_eq!(散的.hit, Some(SearchHit::Description));
}

#[test]
fn 变体锚点上的简介不算主列表这一行的() {
    // 主列表这一行「元数据齐不齐」里的**简介**是按**作品锚点**判的
    // （`Catalog::fill_missing`），年份那一列也是。搜索挑另一条口径的话，
    // 屏上一行写着「缺简介」，搜索却说它「简介里提到它」——同一屏上两句话打架。
    let catalog = 建库();
    assert!(
        搜词(&catalog, 变体简介词).is_empty(),
        "写在变体锚点上的简介居然让主列表那一行命中了",
    );

    // **数据真的在库里**——不是 fixture 写漏了。**规则语言那一侧口径不同**
    // （两个锚点合起来看，因为规则选的是**变体**），它找得到。
    // 两条口径的分野是故意的，这两句话摆在一起才说得清。
    let 按规则 = WorkQuery {
        rule: Some(Rule::parse(&format!("简介~{变体简介词}")).expect("读得懂")),
        ..WorkQuery::default()
    };
    assert_eq!(
        catalog.work_total(&按规则).expect("数得出来"),
        1,
        "规则那条口径该找得到它，否则上面那句「不算」证明不了什么",
    );
}

#[test]
fn 中文与拉丁文的搜索词都工作而繁简差异照现有匹配层的口径() {
    let catalog = 建库();
    assert!(!搜词(&catalog, 中文词).is_empty(), "中文搜索词搜不出东西");
    assert!(!搜词(&catalog, 拉丁词).is_empty(), "拉丁搜索词搜不出东西");

    // 繁体的词找得到繁体的名字——中文这条路两侧都通。
    let 繁 = 搜词(&catalog, "彈頭");
    assert_eq!(
        繁.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
        vec!["合金彈頭 3"],
    );

    // **繁简折叠还没做**（挂账 D121，见挂单 Q104）：简体的「弹头」碰不到繁体的
    // 「合金彈頭 3」。这一条钉的是**现状**不是目标——匹配算法那条线做掉折叠那天，
    // 这里会红，那正是它该红的时候。
    assert!(
        !搜词(&catalog, 中文词)
            .iter()
            .any(|(name, _)| name == "合金彈頭 3"),
        "繁简折叠居然生效了——那是挂账 D121 的活，落地时把这条断言一起改掉",
    );
}

#[test]
fn 搜索与筛选叠加时结果既满足条件又按质量排() {
    let catalog = 建库();
    // 先筛：一条规则把「合金弹头 2」挡在外面（作品名不以「合金」开头的才要）。
    let rule = Rule::parse("都不(作品^合金)").expect("读得懂这条规则");
    let 只筛 = WorkQuery {
        rule: Some(rule.clone()),
        ..WorkQuery::default()
    };
    let 筛出来的: Vec<String> = catalog
        .work_page(&只筛, 0, 64)
        .expect("取得出一页")
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert!(!筛出来的.contains(&"合金弹头 2".to_string()), "规则没起作用");

    // 再搜：结果**既满足条件**（是筛出来那批的子集）**又按质量排**。
    let 又搜 = WorkQuery {
        rule: Some(rule),
        search: 中文词.to_string(),
        ..WorkQuery::default()
    };
    let 结果 = 搜(&catalog, &又搜);
    assert!(!结果.is_empty(), "先筛后搜一条都不剩");
    let mut 上一档 = SearchHit::TitleStart;
    for (name, hit) in &结果 {
        assert!(筛出来的.contains(name), "「{name}」不满足筛选条件却出现了");
        assert!(*hit >= 上一档, "先筛之后次序就不按匹配质量了：{结果:?}");
        上一档 = *hit;
    }
    // 被规则挡掉的那一条，光看搜索是命中的——所以这一趟真的是两边叠加。
    assert!(
        搜词(&catalog, 中文词)
            .iter()
            .any(|(name, _)| name == "合金弹头 2"),
    );
    assert!(!结果.iter().any(|(name, _)| name == "合金弹头 2"));
}

#[test]
fn 存成子库时只带条件不带顺序() {
    // **验收第 5 条**。两半各钉一遍。
    let 只有条件 = WorkQuery {
        rule: Some(Rule::parse("平台=GB").expect("读得懂")),
        ..WorkQuery::default()
    };
    let 期望 = 只有条件.to_rule().expect("写得成").expect("不是空的").text;
    assert_eq!(期望, "平台=GB");

    // 一、**排的哪一列、正着还是倒着，一个字都不进规则**——`to_rule` 连读都不读。
    for order in WorkOrder::ALL {
        for descending in [false, true] {
            let query = WorkQuery {
                order,
                descending,
                ..只有条件.clone()
            };
            assert_eq!(
                query.to_rule().expect("写得成").expect("不是空的").text,
                期望,
                "换个排法（{}、{}）折出来的规则就变了",
                order.label(),
                if descending { "倒着" } else { "正着" },
            );
        }
    }

    // 二、**搜索框有字时当场挡住**（挂单 Q70），不是悄悄少写一条——少一条，
    //     子库选出来的就比屏上多。
    let 搜着 = WorkQuery {
        search: 中文词.to_string(),
        ..只有条件.clone()
    };
    assert_eq!(搜着.to_rule(), Err(Unruly::Search));
    assert!(
        Unruly::Search.advice().contains("搜索"),
        "挡住之后得说清是哪一条以及怎么办",
    );
    // 只打了空白等于没搜：那不该把「存成子库」堵死。
    let 只有空白 = WorkQuery {
        search: "   ".to_string(),
        ..只有条件.clone()
    };
    assert_eq!(
        只有空白.to_rule().expect("写得成").expect("不是空的").text,
        期望,
    );
}

#[test]
fn 搜索着的时候年份那一列照样读得对() {
    // 主列表那条查询里，**匹配质量那一列的参数排在年份那张 join 前面**
    // （`SELECT` 在 `FROM` 之前）。两处绑串了不会报错，只会**静默读出别的值**
    // ——所以这一条比对的是「搜着」与「不搜」两趟读回来的年份。
    let catalog = 建库();
    let 不搜: std::collections::BTreeMap<String, Option<String>> = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页")
        .into_iter()
        .map(|row| (row.name, row.year))
        .collect();
    let 搜着 = catalog
        .work_page(
            &WorkQuery {
                search: 中文词.to_string(),
                ..WorkQuery::default()
            },
            0,
            64,
        )
        .expect("取得出一页");
    assert!(!搜着.is_empty(), "这一趟该搜得出东西");
    let mut 读到过年份 = false;
    for row in &搜着 {
        assert_eq!(
            row.year,
            不搜[&row.name],
            "「{}」搜着的时候年份读出来变了",
            row.name,
        );
        读到过年份 |= row.year.is_some();
    }
    assert!(读到过年份, "这份 fixture 里该有年份可读，否则这条断言等于没测");
}

#[test]
fn 搜索着翻页也不漏行不重行() {
    // 匹配质量是第一把键，而同一档里并列的行一大片——次序不定死，翻页就会漏行与重行，
    // 而人正照着这张表按批量操作。
    let catalog = 建库();
    let query = WorkQuery {
        search: 中文词.to_string(),
        ..WorkQuery::default()
    };
    let total = catalog.work_total(&query).expect("数得出总数");
    let 一次全取: Vec<WorkAnchor> = catalog
        .work_page(&query, 0, 64)
        .expect("取得出一页")
        .into_iter()
        .map(|row| row.anchor)
        .collect();
    assert_eq!(一次全取.len() as u64, total, "总数与取回来的行数对不上");

    let mut 一页页翻: Vec<WorkAnchor> = Vec::new();
    let mut offset = 0;
    while offset < total {
        let rows = catalog.work_page(&query, offset, 2).expect("取得出一页");
        assert!(!rows.is_empty(), "还没到总数就取不出行了");
        一页页翻.extend(rows.into_iter().map(|row| row.anchor));
        offset += 2;
    }
    assert_eq!(一页页翻, 一次全取, "一页页翻完与一次全取不是同一串");
}

#[test]
fn 没搜索的时候一行都不带命中档() {
    // 没打字就没有「匹配质量」这回事：摆一个默认档出去，界面上会印出一句
    // 凭空的「标题以它开头」。
    let catalog = 建库();
    let rows = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    assert_eq!(rows.len() as u64, 总行数(), "不搜的时候该是整个库");
    assert!(rows.iter().all(|row| row.hit.is_none()));

    // **只打了空白等于没搜**：一行都不该被挡掉，也不该算成「换了一批」——
    // 后者会把人勾了两百行的选中当场清掉（`Screen::sync_window` 拿 `same_filter` 判）。
    let 空白 = WorkQuery {
        search: "  ".to_string(),
        ..WorkQuery::default()
    };
    assert_eq!(
        catalog.work_total(&空白).expect("数得出来"),
        rows.len() as u64,
    );
    assert!(
        空白.same_filter(&WorkQuery::default()),
        "多打一个空格居然算换了一批行",
    );
}
