//! 中立库里的**旁路快照**与**首选变体**裁决。
//!
//! ## 快照为什么必须落在中立库里
//!
//! 无损往返靠的不是格式本身，是导入时留下的那份原文（ADR-0003 修订段）。这份原文
//! 要跨进程、跨机器活下来——今天导入、下个月导出，中间程序重启过几十次。放内存里
//! 等于没有；放主库旁边等于往只读的主库里写（ADR-0004）。所以它落在中立库里，
//! 与别的事实一起。
//!
//! 存的是**字节**不是文本：一份 GBK 的元数据文件我们拒绝解析（见
//! [`AdapterError::NotUtf8`](crate::adapter::AdapterError::NotUtf8)），但**能存**——
//! 而「原样存着、说清读不动」比「猜一个编码然后改写用户的原文」诚实得多。
//!
//! ## 哈希是为了认出**外面被人改过了**
//!
//! ADR-0001 的修订段把字段级三方合并降级成了改动检测：所有编辑收敛到工具内，
//! 但工具仍要**检测外部改动并警告**，防止手改成果被静默吞掉。判据就是这一列——
//! 上次我们写出去/读进来的那份是这个哈希，现在盘上那份不是，那就是有人在外面动过。
//!
//! ## 首选变体的裁决为什么单独一张表
//!
//! **首选变体**（汉化 > 官中 > 日版 > 其他）是一条规则，而规则要能被人推翻
//! （ADR-0012 明说「首选这个属性本身需要能被裁决修改」）。裁决是**沉淀**：
//! 它不随重跑识别消失，也不随重新成型消失——这与 `title` 表里 `source = 裁决`
//! 的那些行、`shaping_override` 那张表是同一条纪律。
//!
//! 锚点是 `(作品名, 平台)` 而不是行号，理由与 `scrape_value`、`title` 一样：
//! 重跑识别会把发行版整批换掉；作品那张表票 parking-3/10 起按名字复用、行号跨重跑不换，
//! 但一行收得掉的那几种情形照旧在（底下最后一个变体没了、这一趟一条判据都没落在它身上）。
//! 挂在行号上的东西那时就成了孤儿。

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError};

/// 旁路快照与首选变体裁决的表。
pub(super) const FRONTEND_SCHEMA: &str = "\
-- 一份前端元数据文件的**旁路快照**。无损往返的全部依据。
CREATE TABLE IF NOT EXISTS frontend_snapshot(
    -- 哪个适配器（`Pegasus`）。同一份文件不会被两个格式同时认领，但键上带着它，
    -- 加第二个适配器（票 17 的 ES gamelist）时不必动这张表。
    format TEXT NOT NULL,
    -- 那份文件在哪：绝对路径的 **NFC 形式**（ADR-0020）。
    --
    -- ⚠️ **它是身份，不是访问路径。** ADR-0020 那句「读盘用系统给的原始路径，
    -- 入库与比较用 NFC 形式，两者不能混用」在这里是硬约束：macOS 上 NTFS 交出来的
    -- 名字是 NFD，拿这一列去 `fs::write` 会在旁边**新建一个 NFC 名字的文件**，
    -- 而维护者的原件还躺在原处一个字没改——两份从此各走各的。碰盘一律走
    -- `read_dir` 交出来的那个路径。
    path   TEXT NOT NULL,
    -- **原文逐字节。** 存 BLOB 不存 TEXT：读不动的编码也照样存得住。
    bytes  BLOB NOT NULL,
    -- 内容哈希。**外部改动检测**靠它，不靠时间戳——时间戳会被复制、同步与解压重置。
    hash   TEXT NOT NULL,
    -- 这份快照是**导入**读来的还是**导出**写出去的。
    --
    -- ⚠️ **它在主键里**，于是同一份文件的这两种快照并存。这不是冗余：`导入` 那一行是
    -- **维护者多年手工维护的原件**，是这张票的首要交付；`导出` 那一行只是「我们上次
    -- 写出去的样子」，用来认出外面有没有人动过。两者共用一把主键的话，第一次导出就把
    -- 原件顶掉了——那正是 ADR-0001 说「这个代价不可接受」的那件事，而且顶掉之后
    -- 盘上那份也已经被我们改写，两处都没了。
    origin TEXT NOT NULL,
    at     INTEGER NOT NULL,
    PRIMARY KEY (format, path, origin)
) STRICT;

-- **首选变体**的人工裁决：这个作品在这个平台上，默认启动的是这一个。
-- 规则（汉化 > 官中 > 日版 > 其他）算出来的那个由导出现折，**不落库**；
-- 这里只存人推翻规则的那些。
CREATE TABLE IF NOT EXISTS preferred_variant(
    work        TEXT NOT NULL,
    platform    TEXT NOT NULL,
    variant_key TEXT NOT NULL,
    PRIMARY KEY (work, platform)
) STRICT;
";

/// 快照是怎么来的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotOrigin {
    /// 导入时从维护者的文件读来的。**这一份是他多年手工维护的成果。**
    Imported,
    /// 导出时我们自己写出去的。下次导出拿它比对，认出外面有没有人动过。
    Exported,
}

impl SnapshotOrigin {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Imported => "导入",
            Self::Exported => "导出",
        }
    }

    /// 从词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "导入" => Self::Imported,
            "导出" => Self::Exported,
            _ => return None,
        })
    }
}

/// 一份存下来的快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRow {
    /// 哪个格式。
    pub format: String,
    /// 那份文件在哪。
    pub path: String,
    /// 原文逐字节。
    pub bytes: Vec<u8>,
    /// 内容哈希。
    pub hash: String,
    /// 怎么来的。
    pub origin: SnapshotOrigin,
    /// 什么时候。
    pub at: i64,
}

/// 一串字节的内容哈希。**快照与盘上那份用同一个函数算**，各写一遍必然有一天写岔。
#[must_use]
pub fn hash_of(bytes: &[u8]) -> String {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    context.update(bytes);
    crate::scrape::pool::hex(context.finish().as_ref())
}

impl Catalog {
    /// 存一份快照。同一个 `(格式, 路径)` 写两次是覆盖——快照记的永远是**最近一次**
    /// 我们与那份文件对齐的样子。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_snapshot(
        &mut self,
        format: &str,
        path: &str,
        bytes: &[u8],
        origin: SnapshotOrigin,
    ) -> Result<(), CatalogError> {
        let at = super::now_secs();
        self.conn
            .execute(
                "INSERT INTO frontend_snapshot(format, path, bytes, hash, origin, at)
                 VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(format, path, origin) DO UPDATE SET
                    bytes = excluded.bytes, hash = excluded.hash, at = excluded.at",
                params![format, path, bytes, hash_of(bytes), origin.label(), at],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 一份文件**最近一次**对齐的样子；没有就是 `None`。
    ///
    /// 「最近一次」是外部改动检测要的那一份：盘上那份与它不一致，就是有人在外面动过。
    /// 要维护者的原件走 [`Catalog::imported_snapshot`]。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn snapshot(&self, format: &str, path: &str) -> Result<Option<SnapshotRow>, CatalogError> {
        self.conn
            .query_row(
                &format!("{SELECT_SNAPSHOT} WHERE format = ?1 AND path = ?2 {LATEST_FIRST}"),
                params![format, path],
                read_snapshot,
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// **维护者的原件**：导入时逐字节存下来的那一份。它永远不被导出顶掉。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn imported_snapshot(
        &self,
        format: &str,
        path: &str,
    ) -> Result<Option<SnapshotRow>, CatalogError> {
        self.conn
            .query_row(
                &format!("{SELECT_SNAPSHOT} WHERE format = ?1 AND path = ?2 AND origin = ?3"),
                params![format, path, SnapshotOrigin::Imported.label()],
                read_snapshot,
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// 盘上这串字节，是我们**对齐过的某一份**吗。
    ///
    /// 问「是不是任意一份」而不是「是不是最近那一份」：导入过又导出过的文件在库里有两行，
    /// 而维护者可能把导出产物换回了自己的原件——那不是「有人在外面动过」，
    /// 那是他手上本来就有的东西。把这一态报成冲突，用户每趟都要 `--force` 一次，
    /// 而 `--force` 一旦成了习惯，这道守卫就白设了。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn snapshot_matches(
        &self,
        format: &str,
        path: &str,
        hash: &str,
    ) -> Result<bool, CatalogError> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM frontend_snapshot
                 WHERE format = ?1 AND path = ?2 AND hash = ?3",
                params![format, path, hash],
                |row| row.get(0),
            )
            .map_err(|source| self.err(source))?;
        Ok(count > 0)
    }

    /// 这个格式的每份文件**最近一次**对齐的样子，按路径排。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn snapshots(&self, format: &str) -> Result<Vec<SnapshotRow>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(&format!(
                "{SELECT_SNAPSHOT} WHERE format = ?1
                 AND at = (SELECT MAX(at) FROM frontend_snapshot s
                           WHERE s.format = frontend_snapshot.format AND s.path = frontend_snapshot.path)
                 GROUP BY path ORDER BY path"
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![format], read_snapshot)
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 记一条**首选变体**裁决：这个作品在这个平台上默认启动这一个。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_preferred_variant(
        &mut self,
        work: &str,
        platform: &str,
        variant_key: &str,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO preferred_variant(work, platform, variant_key) VALUES(?1,?2,?3)
                 ON CONFLICT(work, platform) DO UPDATE SET variant_key = excluded.variant_key",
                params![work, platform, variant_key],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 全部首选变体裁决：`(作品名, 平台)` → 变体的键。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn preferred_variants(
        &self,
    ) -> Result<std::collections::BTreeMap<(String, String), String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT work, platform, variant_key FROM preferred_variant")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut out = std::collections::BTreeMap::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let work: String = row.get(0).map_err(|source| self.err(source))?;
            let platform: String = row.get(1).map_err(|source| self.err(source))?;
            let key: String = row.get(2).map_err(|source| self.err(source))?;
            out.insert((work, platform), key);
        }
        Ok(out)
    }

    /// 变体成员按身份各有几个。**附属内容有多少**要从这里数出来——
    /// 它们入库但不导出为前端条目（ADR-0013），报告得说得出被挡下的是多少。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn member_role_counts(
        &self,
    ) -> Result<std::collections::BTreeMap<String, u64>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT role, COUNT(*) FROM variant_member GROUP BY role")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut out = std::collections::BTreeMap::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let role: String = row.get(0).map_err(|source| self.err(source))?;
            let count: i64 = row.get(1).map_err(|source| self.err(source))?;
            out.insert(role, u64::try_from(count).unwrap_or(0));
        }
        Ok(out)
    }

    /// 主文件身份**不是「主文件」**的那些变体：变体的键 → 那个身份。
    ///
    /// 正常情况下一个变体的主文件身份就是主文件，这张表是空的。它存在是因为
    /// **导出要挡下整个都是附属内容的变体**（ADR-0013）——而挡不挡得住不能靠
    /// 「眼下没有这种变体」这句话，得靠一条查得出来的判据。
    ///
    /// 用一句聚合而不是逐个变体查成员：真库里变体有 46,444 个。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn abnormal_main_members(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT v.key, m.role FROM variant v
                 JOIN variant_member m ON m.key = v.main_key
                 WHERE m.role <> '主文件'",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut out = std::collections::BTreeMap::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let role: String = row.get(1).map_err(|source| self.err(source))?;
            out.insert(key, role);
        }
        Ok(out)
    }
}

const SELECT_SNAPSHOT: &str = "SELECT format, path, bytes, hash, origin, at FROM frontend_snapshot";

/// 同一份文件的两行里，取**后写的**那一行。
///
/// 时间戳只到秒，导入完接着导出常常落在同一秒里，所以还要一道确定的分先后：
/// **同一秒里导出赢**——那一份才是我们刚写到盘上的样子。
const LATEST_FIRST: &str = "ORDER BY at DESC, CASE origin WHEN '导出' THEN 0 ELSE 1 END LIMIT 1";

fn read_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotRow> {
    let origin: String = row.get(4)?;
    Ok(SnapshotRow {
        format: row.get(0)?,
        path: row.get(1)?,
        bytes: row.get(2)?,
        hash: row.get(3)?,
        origin: SnapshotOrigin::from_label(&origin).unwrap_or(SnapshotOrigin::Imported),
        at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 快照原样存原样取_包括读不动的编码() {
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        // GBK 的字节。解析器会拒绝它，但**存得住**——原样留着比猜一个编码诚实。
        let bytes = vec![0xC4, 0xE3, 0xBA, 0xC3];
        catalog
            .put_snapshot(
                "Pegasus",
                "/库/metadata.pegasus.txt",
                &bytes,
                SnapshotOrigin::Imported,
            )
            .expect("存得进");
        let row = catalog
            .snapshot("Pegasus", "/库/metadata.pegasus.txt")
            .expect("读得出")
            .expect("有这一份");
        assert_eq!(row.bytes, bytes);
        assert_eq!(row.origin, SnapshotOrigin::Imported);
        assert_eq!(row.hash, hash_of(&bytes));
    }

    #[test]
    fn 导出不顶掉维护者的原件() {
        // **这张票的首要交付。** 第一次导出之后，原件在盘上已经被我们改写；
        // 它若在库里也被顶掉，那就是两处都没了——ADR-0001 说的正是这个代价不可接受。
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let 路径 = "/库/metadata.pegasus.txt";
        catalog
            .put_snapshot(
                "Pegasus",
                路径,
                "# 原件\n".as_bytes(),
                SnapshotOrigin::Imported,
            )
            .expect("存得进");
        catalog
            .put_snapshot(
                "Pegasus",
                路径,
                b"game: A
",
                SnapshotOrigin::Exported,
            )
            .expect("存得进");

        let 原件 = catalog
            .imported_snapshot("Pegasus", 路径)
            .expect("读得出")
            .expect("原件还在");
        assert_eq!(
            原件.bytes,
            "# 原件
"
            .as_bytes(),
            "原件一个字节都没被顶掉"
        );

        let 最近 = catalog
            .snapshot("Pegasus", 路径)
            .expect("读得出")
            .expect("有");
        assert_eq!(
            最近.origin,
            SnapshotOrigin::Exported,
            "外部改动检测比的是**最近一次对齐的样子**"
        );
        assert_eq!(catalog.snapshots("Pegasus").expect("读得出").len(), 1);
    }

    #[test]
    fn 哈希认得出外面被人改过了() {
        let a = hash_of(b"game: A\n");
        let b = hash_of(b"game: B\n");
        assert_ne!(a, b);
        assert_eq!(a, hash_of(b"game: A\n"), "同一份内容永远同一个哈希");
    }

    #[test]
    fn 首选变体的裁决存得住也改得动() {
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        catalog
            .set_preferred_variant("Contra", "FC", "FC/魂斗罗(汉化).zip")
            .expect("写得进");
        catalog
            .set_preferred_variant("Contra", "FC", "FC/Contra (Japan).zip")
            .expect("改得动");
        let all = catalog.preferred_variants().expect("读得出");
        assert_eq!(
            all.get(&("Contra".to_string(), "FC".to_string()))
                .map(String::as_str),
            Some("FC/Contra (Japan).zip"),
            "人推翻规则之后就该是人说的那个"
        );
    }
}
