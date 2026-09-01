//! **媒体池**：按内容哈希存放全部封面、截图、视频的中央仓库，位于本机（ADR-0009）。
//!
//! ## 文件名不是媒体的主键
//!
//! 各前端的媒体目录互不兼容，而且**匹配键在格式之间横跳**：Pegasus 按 ROM 文件名、
//! ES-DE 按文件名、RetroArch 按净化后的 label、Batocera 按文件名加后缀。净化函数
//! **多对一不可逆**——两个不同的标题净化成同一个 label 之后，谁也说不出那张图原本是谁的。
//!
//! 于是媒体一律按**内容哈希**入池，映射只存在中立库里，导出时再按目标格式铺设。
//! 「同一份媒体被多个条目引用时只存一份」因此不是另加的去重步骤，而是内容寻址天然给的：
//! 两个条目引用同一串字节，算出来就是同一个哈希，落在同一个文件上。
//!
//! ## 布局
//!
//! ```text
//! <池>/ab/abcdef…0123.jpg      内容哈希的前两位分一层目录
//! <池>/tmp/…                   算哈希时的落脚处，算完就改名进去
//! ```
//!
//! 分一层目录是给文件系统留余地：一个目录里几十万个文件在很多文件系统上会明显变慢。
//!
//! **扩展名以中立库里记的那一个为准**：同一串字节以 `.jpg` 与 `.jpeg` 两个名字出现时
//! 哈希是同一个，各按各的扩展名落盘就成了两个文件，「只存一份」当场失效。
//!
//! ## 为什么是 SHA-256 而不是识别那一层的 CRC-32
//!
//! CRC-32 是**校验**不是**指纹**：32 位在几万份媒体上撞一次的概率已经不能忽略，而池是
//! 内容寻址的——撞一次就是一张封面悄悄变成另一张。识别那一层用 CRC-32 是因为 DAT 用它、
//! 而且撞上之后还有大小与 DAT 记录兜底；池这里没有第二道判据。

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ring::digest::{Context, SHA256};

use crate::catalog::Catalog;
use crate::fs::LibraryFs;
use crate::path;

/// 媒体池读写出错。
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// 目录建不出来，或者文件落不进去。
    #[error("媒体池写不了：{path}（{source}）")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: io::Error,
    },
}

/// 一份内容读进池里的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ingested {
    /// 哈希是从中立库里直接取回来的，**一个字节都没读盘**。
    Reused {
        /// 内容哈希。
        hash: String,
    },
    /// 读了盘、算了哈希。
    Stored {
        /// 内容哈希。
        hash: String,
        /// 读了多少字节。
        bytes: u64,
        /// 池里此前没有这份内容吗。`false` 就是**去重命中**——算出来发现已经有了，
        /// 池里仍然只有一个文件。
        fresh: bool,
    },
    /// **超过了单份媒体的上限。** 文件好好的，是用户设了上限——
    /// 与「读不动」是平行的两件事，混成一个数，报告就说不出「跳过的那些到底怎么了」
    /// （`CONTEXT.md` 分开 **穿不透** 与 **不可读** 是同一个道理）。
    TooBig {
        /// 已知的字节数；库里没记就是 `None`。
        bytes: Option<u64>,
    },
    /// **读不动。** ADR-0021 的第三态在媒体这一侧的样子：条目在库里，字节取不到。
    ///
    /// **只有这一档是那个第三态。** 「闸门不许下」与「服务端说没有」都不是读不动，
    /// 它们各有自己的变体——混进来，报告就会指着一块好好的盘说它读不动。
    Unreadable {
        /// 为什么。
        why: String,
    },
    /// **取数闸门不许下这一个。** URL 指向白名单之外，而那是**永久**的判断：
    /// 下一趟还是同一个答案，所以它不该让这一对反复重采。
    Refused {
        /// 哪个 URL，闸门怎么说的。
        why: String,
    },
    /// **源说这一份没有。** 一条结论，不是失败——当成失败的话，下一趟会为同一张
    /// 不存在的图再花一份配额。
    Missing {
        /// 哪一个。
        why: String,
    },
    /// 这个扩展名不当成媒体收。防御性的一档——本地媒体源本来就按扩展名筛过一遍了。
    NotMedia {
        /// 哪一个。
        key: String,
    },
}

/// 媒体池。
#[derive(Debug, Clone)]
pub struct MediaPool {
    root: PathBuf,
}

/// 临时文件的编号。同一个进程里连着收几百份媒体，名字不能撞。
static TEMP: AtomicU64 = AtomicU64::new(0);

impl MediaPool {
    /// 打开（必要时新建）一个媒体池。
    ///
    /// # Errors
    /// 目录建不出来时返回错误。
    pub fn open(root: &Path) -> Result<Self, PoolError> {
        let pool = Self {
            root: root.to_path_buf(),
        };
        mkdir(&pool.root)?;
        mkdir(&pool.tmp())?;
        Ok(pool)
    }

    /// 池在哪。
    #[must_use]
    pub fn location(&self) -> &Path {
        &self.root
    }

    /// 一份媒体在池里的落点。
    #[must_use]
    pub fn path_of(&self, hash: &str, ext: &str) -> PathBuf {
        let shard = hash.get(..2).unwrap_or("00");
        self.root.join(shard).join(format!("{hash}.{ext}"))
    }

    /// 池里有这一份吗。
    #[must_use]
    pub fn contains(&self, hash: &str, ext: &str) -> bool {
        self.path_of(hash, ext).is_file()
    }

    fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// 把临时文件改名进池里。**已经有了就删掉临时文件**——那正是「只存一份」。
    fn adopt(&self, temp: &Path, hash: &str, ext: &str) -> Result<bool, PoolError> {
        let target = self.path_of(hash, ext);
        if target.is_file() {
            let _ = std::fs::remove_file(temp);
            return Ok(false);
        }
        if let Some(parent) = target.parent() {
            mkdir(parent)?;
        }
        std::fs::rename(temp, &target).map_err(|source| PoolError::Io {
            path: path::display(&target),
            source,
        })?;
        Ok(true)
    }
}

fn mkdir(dir: &Path) -> Result<(), PoolError> {
    std::fs::create_dir_all(dir).map_err(|source| PoolError::Io {
        path: path::display(dir),
        source,
    })
}

/// 认得出的媒体扩展名，以及它们**规范化之后**叫什么。
///
/// `jpeg` 折成 `jpg` 是有代价可算的：不折的话同一串字节在池里会有两个名字。
const NORMALIZED: &[(&str, &str)] = &[
    ("jpg", "jpg"),
    ("jpeg", "jpg"),
    ("png", "png"),
    ("gif", "gif"),
    ("webp", "webp"),
    ("bmp", "bmp"),
    ("tga", "tga"),
    ("mp4", "mp4"),
    ("mkv", "mkv"),
    ("webm", "webm"),
    ("mov", "mov"),
    ("avi", "avi"),
];

/// 这个扩展名是媒体吗，规范化之后叫什么。
#[must_use]
pub fn normalized_ext(ext: &str) -> Option<&'static str> {
    let lower = ext.to_ascii_lowercase();
    NORMALIZED
        .iter()
        .find(|(name, _)| *name == lower)
        .map(|(_, canonical)| *canonical)
}

/// 一次读取的块大小。64 KiB 是「系统调用次数」与「内存占用」之间的常规折中。
const CHUNK: usize = 64 * 1024;

/// 把主库里的一份媒体收进池里。
///
/// **主库只读**（ADR-0004）：这里只 `open` 加顺序读，一个字节都不写回去。
///
/// # Errors
/// 中立库读写不了、或者池写不进时返回错误。**没收进来不是错误**——那是
/// [`Ingested`] 的三个「没收」变体（[`TooBig`](Ingested::TooBig) /
/// [`Unreadable`](Ingested::Unreadable) / [`NotMedia`](Ingested::NotMedia)），
/// 那一份跳过，整趟继续。三者分开，正因为它们不是同一件事。
pub fn ingest(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    pool: &MediaPool,
    root: &Path,
    claim: &Claim<'_>,
) -> Result<Ingested, super::ScrapeError> {
    let key = claim.key;
    let Some(ext) = normalized_ext(extension_of(key)) else {
        return Ok(Ingested::NotMedia {
            key: key.to_string(),
        });
    };
    // 算过的哈希留着，**连同它的字节数**。**读过的盘不白读**：一块 8.60 TiB 的盘上，
    // 第二趟不该再读一遍（挂账 D14 在识别那一侧兑现过一次，这里是同一条）。
    let known = catalog.media_blob(key)?;

    // **上限在「复用」之前判，而且判之前先把大小凑齐。** 两件事各有理由：
    //
    // - **先于复用**：调低上限之后，此前收进来的那一份也该被挡在外面。上限说的是
    //   「这一趟要不要它」，不是「要不要现在去读它」。
    // - **凑齐大小**：库里的条目大小读不到时（ADR-0021 的第三态）还有算过的那一份
    //   兜着。两处都没有才只能读了才知道——下面读的时候还有一道兜底。
    let size = claim.bytes.or(known.as_ref().map(|(bytes, _)| *bytes));
    if let (Some(cap), Some(bytes)) = (claim.max_bytes, size)
        && bytes > cap
    {
        return Ok(Ingested::TooBig { bytes: Some(bytes) });
    }

    if let Some((_, hash)) = known {
        let stored = catalog.media_ext(&hash)?;
        if let Some(stored) = stored
            && pool.contains(&hash, &stored)
        {
            return Ok(Ingested::Reused { hash });
        }
    }

    let path = root.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
    let mut reader = match library.open(&path) {
        Ok(reader) => reader,
        Err(source) => {
            return Ok(Ingested::Unreadable {
                why: format!("{key} 打不开（{source}）"),
            });
        }
    };

    let temp = temp_path(pool);
    let mut sink = std::fs::File::create(&temp).map_err(|source| PoolError::Io {
        path: path::display(&temp),
        source,
    })?;
    let mut context = Context::new(&SHA256);
    let mut buf = vec![0_u8; CHUNK];
    let mut bytes = 0_u64;
    loop {
        let got = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(got) => got,
            Err(source) => {
                drop(sink);
                let _ = std::fs::remove_file(&temp);
                return Ok(Ingested::Unreadable {
                    why: format!("{key} 读到一半读不动了（{source}）"),
                });
            }
        };
        bytes += got as u64;
        // 兜底：库里记的大小过期时（文件换过、还没重扫），照样不许超上限。
        if let Some(cap) = claim.max_bytes
            && bytes > cap
        {
            drop(sink);
            let _ = std::fs::remove_file(&temp);
            return Ok(Ingested::TooBig { bytes: claim.bytes });
        }
        context.update(&buf[..got]);
        sink.write_all(&buf[..got])
            .map_err(|source| PoolError::Io {
                path: path::display(&temp),
                source,
            })?;
    }
    sink.flush().map_err(|source| PoolError::Io {
        path: path::display(&temp),
        source,
    })?;
    drop(sink);

    let hash = hex(context.finish().as_ref());
    let fresh = adopt_into(pool, catalog, &temp, &hash, ext, bytes)?;
    catalog.put_media_blob(key, &hash, bytes)?;
    Ok(Ingested::Stored { hash, bytes, fresh })
}

/// 池里的一个临时落脚点。算哈希时先写它，算完改名进池。
fn temp_path(pool: &MediaPool) -> PathBuf {
    pool.tmp().join(format!(
        "{}-{}.part",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ))
}

/// 一份算好哈希的临时文件收尾进池：定扩展名、改名、记库。
///
/// **两条进池的路共用这一段**——主库那份边读边算，在线那份整块下回来，前半截不同，
/// 从这里起一模一样。各写一遍的话，同一张图迟早会在池里躺成两份。
///
/// 扩展名**以库里记的那一个为准**：同一串字节以 `.jpg` 与 `.jpeg` 两个名字进来，
/// 各按各的落盘就成了两个文件，「只存一份」当场失效。
///
/// # Errors
/// 中立库读写不了、或者池写不进时返回错误。
fn adopt_into(
    pool: &MediaPool,
    catalog: &mut Catalog,
    temp: &Path,
    hash: &str,
    ext_hint: &str,
    bytes: u64,
) -> Result<bool, super::ScrapeError> {
    let ext = catalog
        .media_ext(hash)?
        .unwrap_or_else(|| ext_hint.to_string());
    let fresh = pool.adopt(temp, hash, &ext)?;
    catalog.put_media(hash, &ext, bytes)?;
    Ok(fresh)
}

/// 把手里这一串字节收进池里，返回 `(内容哈希, 池里此前没有这份吗)`。
///
/// 在线那一侧走它：媒体是下下来的一整块，不像主库那份可以边读边算。
///
/// # Errors
/// 中立库读写不了、或者池写不进时返回错误。
pub fn store(
    pool: &MediaPool,
    catalog: &mut Catalog,
    bytes: &[u8],
    ext: &str,
) -> Result<(String, bool), super::ScrapeError> {
    let mut context = Context::new(&SHA256);
    context.update(bytes);
    let hash = hex(context.finish().as_ref());
    let temp = temp_path(pool);
    std::fs::write(&temp, bytes).map_err(|source| PoolError::Io {
        path: path::display(&temp),
        source,
    })?;
    let fresh = adopt_into(
        pool,
        catalog,
        &temp,
        &hash,
        normalized_ext(ext).unwrap_or("png"),
        u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    )?;
    Ok((hash, fresh))
}

/// 要收的一份媒体：它是谁、库里说它多大、这一趟的上限是多少。
///
/// 把三样捏成一个结构，是为了让 [`ingest`] 的参数表停在五个以内，也为了让
/// 「先按库里记的大小拦一道」这件事在类型上说得出来——没有 `bytes` 就只能读了才知道。
#[derive(Debug, Clone, Copy)]
pub struct Claim<'a> {
    /// 主库里那份媒体的键。
    pub key: &'a str,
    /// 库里记的字节数；元数据读不到时是 `None`（ADR-0021）。
    pub bytes: Option<u64>,
    /// 单份媒体的上限；`None` 是不设上限。
    pub max_bytes: Option<u64>,
}

/// 一个键的扩展名（不含点）；没有扩展名时是空串。
///
/// `Path::extension` 用不上：中立库的键是**用 `/` 分隔的字符串**（ADR-0020），
/// 在 Windows 上拿去造 `Path` 会按 `\\` 断词。
#[must_use]
pub fn extension_of(key: &str) -> &str {
    let name = path::file_name_of_key(key);
    name.rsplit_once('.').map_or("", |(_, ext)| ext)
}

/// 一串字节的十六进制。**媒体的内容哈希与输入指纹都走它**，各写一遍必然有一天写岔。
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 扩展名规范化把jpeg折成jpg() {
        assert_eq!(normalized_ext("JPEG"), Some("jpg"));
        assert_eq!(normalized_ext("jpg"), Some("jpg"));
        assert_eq!(normalized_ext("nes"), None);
    }

    #[test]
    fn 落点按哈希前两位分片() {
        let pool = MediaPool {
            root: PathBuf::from("/池"),
        };
        assert_eq!(
            pool.path_of("abcdef", "jpg"),
            PathBuf::from("/池/ab/abcdef.jpg")
        );
    }

    #[test]
    fn 哈希是sha256() {
        // FIPS 180-4 的经典测试向量：空串。
        let context = Context::new(&SHA256);
        assert_eq!(
            hex(context.finish().as_ref()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
