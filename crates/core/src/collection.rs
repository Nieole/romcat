//! **合集**与**收藏**：同一套成员关系（票 `gui-redesign/06`）。
//!
//! 一个**合集**是用户自己起名的一组游戏，与平台正交（`CONTEXT.md`、ADR-0011）。
//! **收藏**就是其中名字定死的那一组（[`FAVORITE`]）——词表里那句「合集是一组自己起名的，
//! 收藏是那个默认的一组」在代码里的样子就是这个模块只有一张表、一套函数。
//!
//! 两套写法的代价不是多敲几行：`收藏=是` 与 `合集=通关过的` 会变成两个求值器，
//! 而这份规格里唯一的硬约束是**屏上筛出来的那批与同步真正搬过去的那批必须是同一批**
//! （`catalog::filter` 的模块文档）。多一个求值器就多一处会分家的地方。
//!
//! ## 住哪儿：沉淀库，不是中立库
//!
//! **收藏是用户亲手点的，不可再生。** 中立库整份可再生——结构一变就让人删掉重扫
//! （`catalog::SCHEMA_VERSION`），那是省下一整套迁移代码的便宜买卖；把收藏放进去
//! 等于说「下一次改结构时你那几百颗星归零」。所以成员关系落**沉淀库**
//! （`verdict::Store::join`），走顺序迁移，永不要求删库。
//!
//! 中立库里 `collection` / `collection_variant` 那两张表因此是**投影**：识别跑完
//! 照沉淀库重建一遍（[`project`]），结果与沉淀库一致。筛选下推到 SQL 那一层
//! （`catalog::filter`）读的是投影——它跑在中立库自己的连接上，够不着另一个文件。
//!
//! ## 与 ADR-0006 的关系
//!
//! ADR-0006 说「工具完全不管理用户状态」，而它开头第一个词就是「收藏」。**这一票把
//! 收藏从那条线里摘出来，摘的理由与它当初进去的理由是同一条**：ADR-0006 讲的是
//! **前端里玩出来的历史**——Pegasus 的 `favorites.txt`、ES 的 `gamelist.xml`、
//! LaunchBox 的库文件，各家一个地方，多设备并用时合并语义无解，所以工具既不读也不写、
//! 原样搬运（`adapter::gamelist` 那一条至今一个字没动）。
//!
//! 这里这颗星是**另一样东西**：它长在 romcat 自己的沉淀库里，键是**内容锚**，
//! 一次都不往前端的用户状态文件里写，也一次都不从那里读。两者同名不同物。
//! 词表 `CONTEXT.md` 的**收藏**词条已经把这条界线写死了：「**它不是用户状态**」。
//!
//! ADR-0006 那句「换前端或重建库时，多年积累的收藏会丢失」仍然成立，说的是前端那一份。
//!
//! ## 两种锚，如实分开
//!
//! - **内容锚**（`CRC-32` 加大小）：重扫、改名、挪目录、换根，都还认得出。
//! - **路径锚**：拿不到内容判据的那些（真库里 2,631 个**无判据**变体，
//!   `docs/library-facts.md`）只钉得住本机的那个位置——**挪了位置收藏会飘**。
//!
//! 这与裁决是同一个限制，处置也同一条（ADR-0021）：**说得出「这一条挪了位置还认不认得
//! 出」，比让用户以为每条都认得出强**。所以 [`Applied`] 把两种锚各数一个数交出去，
//! 界面照它写出来。
//!
//! ## 读一半、写一半：因为它是一趟长活
//!
//! 「全选 46,483 行 → ★ 收藏」那一下，**读**那一半要为每个变体折出它该钉哪种锚，
//! 而折一个变体的判据要问几次中立库（`identify::content_prints`）——实测在画帧那条
//! 线程上跑 6.7 秒（挂单 `Q119`）。所以这个模块拆成两半：
//!
//! - [`plan`] 只读，收一个[把手](crate::task::Handle)、报进度、按得停，排上**任务台**；
//! - [`commit`] 只写，两份库各一个事务，在**认领**那一步落（台上那条线拿的是中立库的
//!   只读连接，写不动）。
//!
//! [`add`] 与 [`remove`] 是这两半**接起来**的那个特例：命令行、测试与合成数据走它。
//! 两条路共用同一份说法，不许各算各的。

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{Catalog, CatalogError, KEYS_PER_QUERY, VariantRow};
use crate::identify;
use crate::report::thousands;
use crate::site::Site;
use crate::task::{Cutoff, Halted, Handle};
use crate::verdict::{Anchor, Membership, VerdictError};

/// **收藏**这一组合集叫什么。
///
/// 一处说了算：沉淀库里那些成员关系的 `name`、中立库投影里那个合集的名字、
/// 规则语言 `收藏=是` 折出来的 SQL 里那个参数，三处同一个常量。分开写的话，
/// 「按收藏筛」与「按合集筛收藏」会选出两批不一样的东西。
pub const FAVORITE: &str = "收藏";

/// 合集这件事出的错。
#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    /// 中立库读写失败。
    #[error("中立库读不动：{0}")]
    Catalog(#[from] CatalogError),
    /// 沉淀库读写失败。
    #[error("沉淀库读写不了：{0}")]
    Verdict(#[from] VerdictError),
    /// 合集没名字。
    #[error("合集得有个名字：一组东西没有名字，日后既指不着它也筛不出它。")]
    Nameless,
    /// 被按停了。**读那一半整条只读**，所以这一档停在哪儿都是干净的。
    ///
    /// **这一句想怎么写就怎么写。** 任务台分「停了」与「失败」看的是
    /// [`Cutoff`] 落在哪一支（底下那个 `From` 折的），不是这句话说了什么——
    /// 从前它比的是「那句话正是 `Halted` 那一句」，于是这儿差一个字，屏上就说这一趟
    /// 出了错。
    #[error("整批排锚按停了：读那一半整条只读，沉淀库与中立库一个字都没动。")]
    Halted(#[from] Halted),
}

impl From<CollectionError> for Cutoff {
    /// **被按停不折成一句「失败」。**
    ///
    /// 折的是**支**不是话：`Halted` 那一支进 [`Cutoff::Halted`]（它一个字都不带），
    /// 别的照旧带着自己那句话进 [`Cutoff::Failed`]。界面上那一趟于是记成「停了」，
    /// 而不是让人去找哪儿坏了。
    fn from(error: CollectionError) -> Self {
        match error {
            CollectionError::Halted(_) => Self::Halted,
            error => Self::Failed(error.to_string()),
        }
    }
}

/// 一批变体加进（或移出）一个合集之后的账。
///
/// **两种锚各数一个数**：屏上要写得出「其中 N 个只钉得住本机路径，挪了位置会飘」，
/// 而不是笼统一句「收藏了 M 个」。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Applied {
    /// 钉在**内容锚**上的有几个变体——**重扫、改名、挪目录都认得出**。
    pub content: usize,
    /// 只钉得住**路径锚**的有几个变体：**无判据**的那些，挪了位置会飘。
    pub path: usize,
    /// 给的键里有几个中立库里根本没有。**不静静吞掉**：那说明屏上那份名单已经过期。
    pub missing: usize,
    /// 真正动了几条**成员关系**。本来就在里面（或本来就不在）的不算。
    ///
    /// **它数的是成员关系不是变体**，两者一般相等，只有一种情况不等：取消时同一个变体
    /// 上两种锚都在库里（识别跑过之后它从路径锚升成了内容锚，而先前那条没删），
    /// 那一下拿掉两条。
    pub changed: usize,
}

impl Applied {
    /// 这一趟碰到了几个真的变体。
    #[must_use]
    pub fn touched(&self) -> usize {
        self.content + self.path
    }
}

/// 照沉淀库把中立库里的合集重建一遍之后的账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Projected {
    /// 投影出几个合集。
    pub collections: usize,
    /// 投影出几条「这个变体在这个合集里」。
    pub members: usize,
    /// 有几条成员关系在这份中立库里落不了地——内容锚在这儿没有对应的变体
    /// （盘没插、还没扫到、或者那份内容压根不在这台机器上）。
    ///
    /// **它们一条都没删**：沉淀库不可再生，落不了地不等于不该留着。
    pub unresolved: usize,
}

/// 这个变体眼下钉得住哪一种锚。
///
/// **内容判据拿得到就用内容锚**，拿不到才退路径锚——与裁决同一条规矩
/// （`triage::Item::anchor`），而且必须是同一条：两处各挑各的，同一个变体上的收藏与
/// 裁决会钉在不同的东西上，改个名字丢一个留一个。
///
/// **判据由调用方递进来**，这一层不自己去问：整批那条路一趟就把全批的判据取回来了
/// （[`identify::content_prints`]），在这儿再问一次等于把省下的那笔钱又花回去。
#[must_use]
pub fn anchor_of(
    library: &str,
    variant: &VariantRow,
    print: Option<&identify::ContentPrint>,
) -> Anchor {
    match print {
        Some(print) => Anchor::Content {
            crc32: print.crc32,
            size: print.size,
            sha1: None,
        },
        None => Anchor::Path {
            library: library.to_string(),
            variant_key: variant.key.clone(),
        },
    }
}

/// 一批变体进出一个合集这件事**读完了、还没落库**的那一半。
///
/// 拆成两半是因为它们跑在两条线程上：读那一半排上**任务台**（一个变体要问几次中立库，
/// 全选 46,483 行就是十几万次往返，票 `parking-3/09`），写那一半在**认领**那一步——
/// 台上那条线拿的是中立库的只读连接，写不动（`gui::task::Product` 的文档）。
#[derive(Debug, Clone)]
pub struct Plan {
    /// 往哪个合集里加、或者从哪个合集里拿。**已经去过首尾空白**。
    pub name: String,
    /// 是加进去（`true`）还是拿出来。
    pub joining: bool,
    /// 读那一半数出来的账：两种锚各几个、有几个键中立库里已经没有。
    /// **[`Applied::changed`] 要落库之后才知道**，这里是 0。
    tally: Applied,
    /// 要往沉淀库里写（或者删）的那些锚。
    anchors: Vec<Anchor>,
    /// 中立库那份投影要动的那几个变体键。
    touched: Vec<String>,
}

/// 排一趟：这一批变体各该钉在哪种锚上、投影要动哪几个键。**一个字都不写库。**
///
/// 长入口的形状（收一个[把手](Handle)、报进度、按得停）与别的长活一模一样。
/// 整条只读，所以**停在哪儿都是干净的**——那是[收场](crate::task::Ending)四档里的
/// 「停了，什么都没留下」，不是**停在半路**：再排一次就是从头排一次。
///
/// 判据整批取（[`identify::content_prints`]），不是一个变体问四次库。
///
/// # Errors
/// 名字是空的、中立库读不动、或者被按停时返回错误。
pub fn plan(
    catalog: &Catalog,
    library: &str,
    name: &str,
    keys: &[String],
    joining: bool,
    task: &Handle,
) -> Result<Plan, CollectionError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CollectionError::Nameless);
    }
    let mut tally = Applied::default();
    let mut anchors: Vec<Anchor> = Vec::with_capacity(keys.len());
    // **投影要动的键不等于人点的那几个**：一条内容锚在本机可能落在好几个变体上
    // （两块盘各存一份）。走 [`landed`]——与整份重建那一处同一个展开，
    // 否则屏上这一下与下一趟识别会给出两种样子。
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let total = u64::try_from(keys.len()).unwrap_or(u64::MAX);
    task.steps(1);
    task.step(&format!("为 {} 个变体折锚", thousands(total)))?;
    let mut done = 0_u64;
    // **一段一条 `IN`，一段一次停下的机会**：段与判据那几条查询取的是同一个长度
    // （[`KEYS_PER_QUERY`]），于是「按下停下」到「真的停了」之间最坏就是这一段。
    for chunk in keys.chunks(KEYS_PER_QUERY) {
        task.check()?;
        let want: Vec<&str> = chunk.iter().map(String::as_str).collect();
        let rows = catalog.variants_of(&want)?;
        let by_key: BTreeMap<&str, &VariantRow> =
            rows.iter().map(|row| (row.key.as_str(), row)).collect();
        let prints = identify::content_prints(catalog, &rows)?;
        for key in chunk {
            // **不静静吞掉**：中立库里没有的那些说明屏上那份名单已经过期。
            let Some(variant) = by_key.get(key.as_str()).copied() else {
                tally.missing += 1;
                continue;
            };
            let anchor = anchor_of(library, variant, prints.get(&variant.key));
            match anchor {
                Anchor::Content { .. } => tally.content += 1,
                Anchor::Path { .. } => tally.path += 1,
            }
            if !joining && matches!(anchor, Anchor::Content { .. }) {
                // **两种锚都拿一遍。** 上面那条是「眼下该钉哪种」，而库里存着的可能是
                // 另一种——识别跑过之后同一个变体从路径锚升成了内容锚，正是这种情况。
                // 只拿一种的话星星点不灭。
                let by_path = Anchor::Path {
                    library: library.to_string(),
                    variant_key: variant.key.clone(),
                };
                touched.extend(landed(catalog, &by_path, Some(&variant.key))?);
                anchors.push(by_path);
            }
            touched.extend(landed(catalog, &anchor, Some(&variant.key))?);
            anchors.push(anchor);
        }
        done += u64::try_from(chunk.len()).unwrap_or(0);
        task.tick(done, total);
    }
    Ok(Plan {
        name: name.to_string(),
        joining,
        tally,
        anchors,
        touched: touched.into_iter().collect(),
    })
}

/// 把排好的那一趟落下去：沉淀库一个事务，中立库那份投影一个事务。
///
/// **它不收把手，也停不下来**，那是有意的：两份库各一个事务，中途停下留下的正是
/// 「屏上亮着而沉淀库里没有」那种半截状态。停要停在[排那一步](plan)。
///
/// # Errors
/// 两份库有一份写不动时返回错误。
pub fn commit(site: &mut Site, plan: &Plan) -> Result<Applied, CollectionError> {
    let mut applied = plan.tally;
    // 沉淀库那一半：整批一个事务。
    applied.changed = if plan.joining {
        let memberships: Vec<Membership> = plan
            .anchors
            .iter()
            .map(|anchor| Membership::now(&plan.name, anchor.clone()))
            .collect();
        site.store.join(&memberships)?
    } else {
        site.store.leave(&plan.name, &plan.anchors)?
    };
    // 中立库那一半（投影）跟着改，也是一个事务。**只动碰到的那几个键**：
    // 整份重建（[`project`]）要把全库的成员关系摊一遍，而这一下人是按着按钮等结果的。
    if plan.joining {
        // **一个成员都落不了地就别建那个合集**：那会在筛选栏那一维上留下一个
        // 一件东西都选不出来的合集，而这一票的正题就是把「合集 0 个」收掉。
        if !plan.touched.is_empty() {
            let id = site.catalog.add_collection(&plan.name)?;
            site.catalog.add_all_to_collection(id, &plan.touched)?;
        }
    } else {
        site.catalog
            .remove_all_from_collection(&plan.name, &plan.touched)?;
        // 一条成员都不剩的合集从投影里消失，同上。
        site.catalog.drop_empty_collections()?;
    }
    Ok(applied)
}

/// 把这一批变体放进一个合集。**收藏就是 `name` 取 [`FAVORITE`]。**
///
/// 落沉淀库，同时把中立库那份投影跟着改——**屏上当场生效**要的就是后半句。
/// 整份重建（[`project`]）太贵也没必要：动了哪几个键这里一清二楚。
///
/// **它是[排一趟](plan)加[落下去](commit)的特例**：那一批不大、就地跑完就是。
/// 命令行与测试走这条；界面上「全选 46,483 行」那一下走任务台，两条路共用同一份说法。
///
/// # Errors
/// 名字是空的、或者两份库有一份读写失败时返回错误。
pub fn add(site: &mut Site, name: &str, keys: &[String]) -> Result<Applied, CollectionError> {
    apply(site, name, keys, true)
}

/// 把这一批变体从一个合集里拿出来。
///
/// **两种锚都拿**：一个变体可能是识别之前按路径锚放进去、之后又按内容锚放过一次的，
/// 只拿一种的话星星点不灭。
///
/// # Errors
/// 同 [`add`]。
pub fn remove(site: &mut Site, name: &str, keys: &[String]) -> Result<Applied, CollectionError> {
    apply(site, name, keys, false)
}

fn apply(
    site: &mut Site,
    name: &str,
    keys: &[String],
    joining: bool,
) -> Result<Applied, CollectionError> {
    let library = site.library.clone();
    let planned = plan(&site.catalog, &library, name, keys, joining, &Handle::new())?;
    commit(site, &planned)
}

/// 这个变体在哪几个合集里，各**钉在哪种锚上**。按合集名排。
///
/// 读的是**沉淀库**不是投影：详情面板上那句「挪了位置会飘」的依据只有沉淀库里那条
/// 成员关系说得出来——投影那张表只记「在不在里面」，记不着它靠什么认出来的。
///
/// # Errors
/// 两份库有一份读不动时返回错误。
pub fn standing(site: &Site, key: &str) -> Result<Vec<(String, &'static str)>, CollectionError> {
    let Some(variant) = site.catalog.variant(key)? else {
        return Ok(Vec::new());
    };
    let mut out: BTreeMap<String, &'static str> = BTreeMap::new();
    // 路径锚那一份先问，内容锚后问——同一个合集两种锚都有时，**报内容锚那一种**：
    // 认得出改名的那条成立，整条成员关系就认得出改名。
    let by_path = Anchor::Path {
        library: site.library.clone(),
        variant_key: variant.key.clone(),
    };
    for name in site.store.joined(&by_path)? {
        out.insert(name, crate::verdict::ANCHOR_PATH);
    }
    if let Some(print) = identify::content_print(&site.catalog, &variant)? {
        let by_content = Anchor::Content {
            crc32: print.crc32,
            size: print.size,
            sha1: None,
        };
        for name in site.store.joined(&by_content)? {
            out.insert(name, crate::verdict::ANCHOR_CONTENT);
        }
    }
    Ok(out.into_iter().collect())
}

/// 照沉淀库把中立库里的合集**整份重建**一遍。
///
/// 这是「删掉中立库重扫之后收藏还在」那句话真正的兑现处：识别跑完调它一次
/// （`identify::run` 的末尾），中立库里那两张表就回到与沉淀库一致的样子。
///
/// 一条锚落在哪几个变体上由 [`landed`] 说了算，**增量那一处走的是同一个函数**。
///
/// 走「拿成员关系去找变体」而不是「拿每个变体去问在不在合集里」，理由与
/// `scrape::zh::Rulings::resolve` 同一条：成员关系是**人一条条点出来的**，
/// 量级几百到几千，与四万多个变体不同阶。
///
/// # Errors
/// 读写中立库失败时返回错误。**它一次都不碰沉淀库**——成员关系是调用方拿着的那份
/// 快照（`verdict::Index::memberships`），所以这里的错只可能出自中立库。
pub fn project(
    catalog: &mut Catalog,
    memberships: &[Membership],
) -> Result<Projected, CatalogError> {
    let mut by_name: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut projected = Projected::default();
    for membership in memberships {
        let landed = landed(catalog, &membership.anchor, None)?;
        if landed.is_empty() {
            projected.unresolved += 1;
        }
        by_name
            .entry(membership.name.as_str())
            .or_default()
            .extend(landed);
    }
    let groups: Vec<(&str, Vec<String>)> = by_name
        .into_iter()
        .filter(|(_, keys)| !keys.is_empty())
        .map(|(name, keys)| (name, keys.into_iter().collect()))
        .collect();
    projected.collections = groups.len();
    projected.members = groups.iter().map(|(_, keys)| keys.len()).sum();
    // **先清后写，而且是一个事务。** 投影可以整份丢掉重建，但**中途死掉不许留下半份**
    // ——那时屏上、筛选栏那一维、子库的选择集上所有的星一起消失，而人看不出那是
    // 「沉淀库里没了」还是「重建跑到一半」，只能等下一趟识别。
    catalog.replace_collections(&groups)?;
    Ok(projected)
}

/// 一条锚在这份中立库里**落在哪几个变体上**。
///
/// **`project` 与 `add` / `remove` 共用它，这是硬的**：内容锚钉的是「世上这份内容」，
/// 而同一份内容在本机可以躺在**好几个变体**里（两块盘各存一份，`report::duplicates`
/// 那一层专门数这个）。两处各展开各的，就会出现「屏上还亮着而沉淀库里已经没了」
/// ——增量那一处只动人点的那一个，整份重建那一处动全部，下一趟识别时另一个凭空变样。
///
/// **内容锚那一批要复核一遍**：[`Catalog::variants_with_content`] 交出来的是「某个成员
/// 正好是这份内容」的变体，而成员关系钉的是「代表这个变体的那份内容」。复核走
/// [`identify::content_print`]——那一层才是「谁代表这个变体」的唯一说法
/// （与 `scrape::zh::Rulings::resolve` 同一条路，两处不许各写一遍）。
///
/// `known` 是「这条锚**就是从这个变体身上算出来的**」，那一个不必再复核一遍。
/// 排一趟那条路（[`plan`]）刚刚才为它算过一次判据，而那一次不便宜——折一个变体的判据
/// 要问中立库好几次（[`identify::content_prints`]）。
fn landed(
    catalog: &Catalog,
    anchor: &Anchor,
    known: Option<&str>,
) -> Result<Vec<String>, CatalogError> {
    match anchor {
        Anchor::Content { crc32, size, .. } => {
            let mut out = Vec::new();
            for key in catalog.variants_with_content(*crc32, *size)? {
                if known == Some(key.as_str()) {
                    out.push(key);
                    continue;
                }
                let Some(row) = catalog.variant(&key)? else {
                    continue;
                };
                let Some(print) = identify::content_print(catalog, &row)? else {
                    continue;
                };
                if print.crc32 == *crc32 && print.size == *size {
                    out.push(key);
                }
            }
            Ok(out)
        }
        // **路径锚就是那一个变体**，不必再问一次库：`known` 说的正是「这条锚是从它身上
        // 算出来的」，而那说明它就在库里——排一趟（[`plan`]）刚为它取过那一行。
        Anchor::Path { variant_key, .. } if known == Some(variant_key.as_str()) => {
            Ok(vec![variant_key.clone()])
        }
        Anchor::Path { variant_key, .. } => Ok(catalog
            .variant(variant_key)?
            .map(|row| vec![row.key])
            .unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 收藏这个名字一处说了算() {
        // 这条看着像废话，钉的是别处：`catalog::filter` 那条 SQL、
        // `sublibrary::facts` 那个布尔、界面上那颗星，都得引这一个常量。
        // 谁抄了一份字面量进去，改名字那天就会有一处漏改，而它选出来的是空集。
        assert_eq!(FAVORITE, "收藏");
    }
}
