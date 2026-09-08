//! **媒体预览**：把**媒体池**里那一份变成一张画得出来的图。
//!
//! ## 为什么这一层在核心里
//!
//! 界面只画和转发（ADR-0005）。「解得开吗」「视频抽不抽得出首帧」「抽出来的存哪儿」
//! 这三问全是**领域侧要说清的账**，界面拿到的只该是一片 RGBA 加一个宽高。把它们摊在
//! 画帧那条线程上，代价是明摆着的：真库的媒体池 9.5 GB，一张 4000×3000 的封面解一次
//! 就是几十毫秒，而翻库是一行接一行翻的。
//!
//! ## 视频**不在窗口里解码播放**
//!
//! 那要 ffmpeg 的 Rust 绑定，依赖涨一大截且 Windows 上构建麻烦——而 Windows 正是
//! 主力机（ADR-0018）。这里走的是另一条：**抽一帧首帧当封面，播放交给系统默认程序**
//! （[`open_externally`]）。抽帧调的是**外部 ffmpeg 进程**，
//! **找不到就退化成占位图标**（[`Missing::NoFfmpeg`]）——它不是硬依赖，一台没装
//! ffmpeg 的机器上这个窗口照常可用，只是那 178 个 mp4 显示成占位。
//!
//! ## 抽出来的首帧进**媒体池**
//!
//! 首帧本身就是一份内容，因此照 ADR-0009 按**内容哈希**入池，映射记在中立库的
//! `media_frame` 上（[`Catalog::put_media_frame`](crate::catalog::Catalog::put_media_frame)）。
//! 一份视频抽一次就够了：真库里 178 个 mp4，每开一次详情面板重抽一遍等于每次翻库
//! 都拉起一百多个进程。
//!
//! ## 一条线程在后头解，画帧那条一次都不等
//!
//! [`Loader`] 是这件事的落点：界面每帧问一次「这一份的图好了没」，好了就画，没好就画
//! 占位。它**不是**[`task::Board`](crate::task::Board)——那一张台子一次只跑一趟长活
//! （扫描、刮削），排队等着才是对的；而预览是几十件几毫秒的小活，排进那条队列会把
//! 「停下扫描」那颗按钮挤到一百张缩略图后面去。

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use super::pool::{MediaPool, PoolError, normalized_ext};
use crate::path;

/// 缩略图的**长边**上限，像素。
///
/// 512 是「详情面板里那一格看得清」与「解一张不占太多内存」之间的折中：一张 512×512
/// 的 RGBA 是 1 MiB，一屏摆十几格也就十几兆；而池里那些封面动辄 2000 像素起，
/// 原样搬进显存翻几行就是几百兆。
pub const EDGE: u32 = 512;

/// 抽首帧调的那个程序。
///
/// 单拎出来是为了**测得到「它不在」那条路**：验收要求一台没有 ffmpeg 的机器上照常
/// 可用，而测试不能靠「跑测试这台机器碰巧没装」——那样的测试在装了的机器上就不测了。
pub const FFMPEG: &str = "ffmpeg";

/// 一张画得出来的位图：RGBA8、行优先、**不预乘**（`egui::ColorImage` 要的就是这个）。
#[derive(Clone, PartialEq, Eq)]
pub struct Thumbnail {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

// 手写 `Debug`：`derive` 会把几兆像素整个打出来，一句 `dbg!` 就淹掉整个终端。
impl std::fmt::Debug for Thumbnail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Thumbnail")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

impl Thumbnail {
    /// 多宽。
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 多高。
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// 像素，RGBA8。长度恒是 `宽 × 高 × 4`。
    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// **没有图**，以及说得出的那个原因。
///
/// 一档一句话：**「媒体读不动时说清是哪一件、为什么」**是这张票的一条验收，而把它们
/// 合成一个 `None` 或者一句「加载失败」，人就只能挨个去文件管理器里翻。
/// 这几档之间的分界与 [`Ingested`](super::pool::Ingested) 同源，理由也同源
/// （ADR-0021：**不可读**是独立的第三态，不能跟「没有」混）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// **池里没有这个文件。** 库里记着这条引用，池里那一份不在——导出时它一张也铺不出去。
    NotInPool {
        /// 内容哈希。
        hash: String,
        /// 扩展名。
        ext: String,
    },
    /// **读不动。** 文件在，字节取不到（ADR-0021 的第三态）。
    Unreadable {
        /// 为什么。
        why: String,
    },
    /// **字节在，解不开。** 半截文件、或者名字与内容对不上。
    Undecodable {
        /// 为什么。
        why: String,
    },
    /// **这个格式这一版不解。** 认得出它是张图，但没带这个解码器——
    /// 与「解不开」分开，因为它说的是工具的能力边界，不是这份文件坏了。
    UnknownFormat {
        /// 哪个扩展名。
        ext: String,
    },
    /// **抽首帧要外部 ffmpeg，这台机器上没有。** 不是错误，是这条路本来就允许退化。
    NoFfmpeg {
        /// 找的是哪个程序。
        program: String,
    },
    /// **ffmpeg 在，可这一份抽不出来。** 与「没装」分得开：一个该去装，一个该去看文件。
    FrameFailed {
        /// 为什么。
        why: String,
    },
}

impl Missing {
    /// 排成给人看的一句话。
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::NotInPool { hash, ext } => {
                format!("媒体池里没有 {hash}.{ext} 这个文件")
            }
            Self::Unreadable { why } => format!("读不动：{why}"),
            Self::Undecodable { why } => format!("解不开：{why}"),
            Self::UnknownFormat { ext } => {
                format!("这一版不解 {ext} 这个格式，用系统看图工具打开吧")
            }
            Self::NoFfmpeg { program } => {
                format!("抽首帧要外部 {program}，这台机器上没有——视频照样点得开")
            }
            Self::FrameFailed { why } => format!("首帧抽不出来：{why}"),
        }
    }

    /// 这一档是**这台机器没装 ffmpeg**吗。
    ///
    /// 界面拿它决定那句提示只说一遍还是每一格都说——一台没装的机器上，178 个视频
    /// 各摆一句「没装 ffmpeg」是噪音。
    #[must_use]
    pub fn is_no_ffmpeg(&self) -> bool {
        matches!(self, Self::NoFfmpeg { .. })
    }
}

/// 一份媒体预览出来是什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    /// 解出来了。
    Ready(Box<Thumbnail>),
    /// 没有图，附一句为什么。
    Missing(Missing),
}

impl Preview {
    /// 解出来了吗。
    #[must_use]
    pub fn ready(&self) -> Option<&Thumbnail> {
        match self {
            Self::Ready(thumb) => Some(thumb),
            Self::Missing(_) => None,
        }
    }

    /// 没有图的话，为什么。
    #[must_use]
    pub fn missing(&self) -> Option<&Missing> {
        match self {
            Self::Ready(_) => None,
            Self::Missing(why) => Some(why),
        }
    }
}

/// 这一版**解得开**的图片扩展名（已经过 [`normalized_ext`] 规范化）。
///
/// 只有这两种，是因为真库的媒体池里图片就只有这两种：342 张 jpg、98 张 png
/// （`docs/library-facts.md`）。gif 与 webp 各要再拉一个解码器进来，而库里一张都没有。
const DECODABLE: &[&str] = &["jpg", "png"];

/// 当**视频**看的扩展名（已经过 [`normalized_ext`] 规范化）。
const VIDEO: &[&str] = &["mp4", "mkv", "webm", "mov", "avi"];

/// 这个扩展名这一版解得开吗。
#[must_use]
pub fn decodable(ext: &str) -> bool {
    normalized_ext(ext).is_some_and(|ext| DECODABLE.contains(&ext))
}

/// 这个扩展名当视频看吗。
#[must_use]
pub fn is_video(ext: &str) -> bool {
    normalized_ext(ext).is_some_and(|ext| VIDEO.contains(&ext))
}

/// 一张图**最多解到多大**：像素边长，以及解码时最多分配多少字节。
///
/// **不设上限的话，一张声明 20000×20000 的 PNG 会让后台那条线程一口气要 1.6 GB**
/// ——文件头里那两个数是**别人写的**，而 [`fit`] 是先整张解开再缩，峰值与 `edge` 无关。
/// 分配不下就是 abort，整个窗口跟着走。
///
/// 16384 是常见显卡的纹理边长上限，256 MiB 正好装得下一张 8192×8192 的 RGBA——
/// 真库那些封面最大不过四千像素，这两个数留的余量已经很宽。
const MAX_SIDE: u32 = 16_384;
/// 见 [`MAX_SIDE`]。
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

/// 把一片字节解成缩略图，**长边不超过 `edge`**。
///
/// **只缩不放**：一张 16×16 的 logo 拉到 512 只会糊成一片，而它本来就该按原尺寸画。
///
/// 解码带着上限（`MAX_SIDE`）：文件头里那两个尺寸是别人写的，照单全收会被一张图撑死。
///
/// # Errors
/// 解不开、或者超过上限时返回 [`Missing::Undecodable`]。
pub fn decode(bytes: &[u8], edge: u32) -> Result<Thumbnail, Missing> {
    let undecodable = |source: image::ImageError| Missing::Undecodable {
        why: source.to_string(),
    };
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|source| Missing::Undecodable {
            why: source.to_string(),
        })?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let decoded = reader.decode().map_err(undecodable)?;
    Ok(fit(&decoded, edge))
}

/// 等比缩到长边 `edge` 以内，再摊成 RGBA8。
fn fit(decoded: &image::DynamicImage, edge: u32) -> Thumbnail {
    use image::GenericImageView as _;
    let (width, height) = decoded.dimensions();
    let longest = width.max(height);
    // `thumbnail` 保纵横比，但小图也会被它放大——那正是要躲开的。
    let scaled = if longest > edge && edge > 0 {
        decoded.thumbnail(edge, edge)
    } else {
        decoded.clone()
    };
    let rgba = scaled.to_rgba8();
    Thumbnail {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }
}

/// 池里那一份**图片**解成缩略图。
///
/// 不返回 `Result`：**「没有图」不是错误**，是六种说得出口的情形之一（[`Missing`]）——
/// 一条条列在界面上，比一句「加载失败」有用得多。
#[must_use]
pub fn from_pool(pool: &MediaPool, hash: &str, ext: &str, edge: u32) -> Preview {
    let at = pool.path_of(hash, ext);
    // **先问文件在不在，再问解不解得开。** 反过来的话，一条 `.webp` 的**悬空引用**
    // 会被报成「这一版不解 webp，用系统看图工具打开吧」——人照这句话去开，
    // 那个文件压根不在池里。「东西没了」比「这一版不认得」更要紧，先说它。
    if !at.is_file() {
        return Preview::Missing(Missing::NotInPool {
            hash: hash.to_string(),
            ext: ext.to_string(),
        });
    }
    if !decodable(ext) {
        return Preview::Missing(Missing::UnknownFormat {
            ext: ext.to_string(),
        });
    }
    let bytes = match std::fs::read(&at) {
        Ok(bytes) => bytes,
        Err(source) => {
            return Preview::Missing(Missing::Unreadable {
                why: format!("{}（{source}）", path::display(&at)),
            });
        }
    };
    match decode(&bytes, edge) {
        Ok(thumb) => Preview::Ready(Box::new(thumb)),
        Err(why) => Preview::Missing(why),
    }
}

/// 抽一份视频的**首帧**，返回一整块 PNG 字节。
///
/// 走的是**外部进程**，不是 ffmpeg 的 Rust 绑定——那正是这张票不肯付的那笔依赖
/// （规格的 Out of Scope）。`program` 单独收进来是为了让「它不在」那条路测得到。
///
/// # Errors
/// - 程序不在：[`Missing::NoFfmpeg`]。**这不是失败**，是允许的退化。
/// - 程序在、这一份抽不出来：[`Missing::FrameFailed`]。
pub fn extract_frame(program: &str, video: &Path) -> Result<Vec<u8>, Missing> {
    // `-nostdin` 是防它把终端的标准输入抢走（ffmpeg 默认会读 stdin 收快捷键）；
    // `-v error` 让它别把两屏 banner 写进 stderr，那样出错时那句话才找得着。
    let out = std::process::Command::new(program)
        .arg("-nostdin")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(video)
        .args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "png", "-"])
        .output();
    let out = match out {
        Ok(out) => out,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(Missing::NoFfmpeg {
                program: program.to_string(),
            });
        }
        Err(source) => {
            return Err(Missing::FrameFailed {
                why: format!("{program} 跑不起来（{source}）"),
            });
        }
    };
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why.trim();
        return Err(Missing::FrameFailed {
            why: if why.is_empty() {
                format!("{program} 退出码 {}", out.status)
            } else {
                why.chars().take(200).collect()
            },
        });
    }
    if out.stdout.is_empty() {
        return Err(Missing::FrameFailed {
            why: format!("{program} 一个字节都没吐出来"),
        });
    }
    Ok(out.stdout)
}

/// 用**系统默认程序**打开这个文件。视频那一下点下去走的就是它。
///
/// 三个平台各一条命令，**都不经过 shell**：路径直接当参数递过去，于是带空格、带中文、
/// 带引号的文件名都不必转义（真库里这三样都有）。
///
/// # Errors
/// 调不起来时返回一句给人看的话。**这不该让窗口崩**——一台没配默认播放器的机器上，
/// 该看见的是一行提示。
pub fn open_externally(path: &Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        // `start` 是 `cmd` 的内建命令，不是程序。头一个引号串是**窗口标题**——
        // 少了它，`start "C:\…\片子.mp4"` 会把路径当标题，然后什么都不开。
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    // `spawn` 而不是 `output`：播放器是个长住的窗口，等它退出等于把界面挂在那儿。
    command
        .spawn()
        .map(|_| ())
        .map_err(|source| format!("{} 打不开（{source}）", path::display(path)))
}

/// 要预览的是池里哪一份：内容哈希加扩展名。
pub type Key = (String, String);

/// 后台那条线程刚做完的一件。
#[derive(Debug)]
pub struct Settled {
    /// 问的是哪一份。
    pub key: Key,
    /// 结果。
    pub preview: Preview,
    /// **刚抽出来、已经落进池里的首帧**，等主线程把它记进中立库。
    ///
    /// 落盘那一半在后台做（纯文件系统），记库那一半必须回主线程——
    /// 中立库**全程只有一个写者**（[`Catalog::read_only`](crate::catalog::Catalog::read_only)）。
    pub frame: Option<Frame>,
}

/// 一份刚落池的首帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 它是从哪份视频抽出来的（视频的内容哈希）。
    pub video: String,
    /// 首帧自己的内容哈希。
    pub hash: String,
    /// 首帧多少字节。
    pub bytes: u64,
}

/// 后台那条线程手里的一件活。
enum Job {
    /// 直接解池里这个文件。图片走它，视频的**已知首帧**也走它。
    Decode { key: Key, hash: String, ext: String },
    /// 抽一份视频的首帧，落池，再解。
    Frame {
        key: Key,
        hash: String,
        ext: String,
        program: String,
    },
}

/// **预览加载器**：一条后台线程加一份缓存，画帧那条线程一次都不等。
///
/// 界面每帧两句话：[`drain`](Self::drain) 把跑完的收进来（顺便拿到要记库的首帧），
/// [`want`](Self::want) 问一份图——有就给，没有就悄悄排上队并返回 `None`，
/// 那一帧画占位。
pub struct Loader {
    pool: MediaPool,
    edge: u32,
    program: String,
    /// 往后台递活。
    jobs: Sender<Job>,
    /// 后台交回来的。
    done: Receiver<Settled>,
    /// 缓存：问过的都在这儿，**包括没解出来的那些**——不然一份坏图会被反复重试。
    cache: HashMap<Key, Preview>,
    /// 递出去还没回来的。**防重排**：翻库时同一格每帧都问一次。
    inflight: HashSet<Key>,
    /// 后台线程。`Drop` 时等它收尾。
    worker: Option<JoinHandle<()>>,
    /// 告诉后台线程别再干了。
    quit: Arc<AtomicBool>,
}

impl std::fmt::Debug for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loader")
            .field("pool", &self.pool.location())
            .field("edge", &self.edge)
            .field("program", &self.program)
            .field("cached", &self.cache.len())
            .field("inflight", &self.inflight.len())
            .finish()
    }
}

impl Loader {
    /// 开一个，后台线程当场起来。
    #[must_use]
    pub fn start(pool: MediaPool, edge: u32) -> Self {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, done) = channel::<Settled>();
        let quit = Arc::new(AtomicBool::new(false));
        let worker = {
            let pool = pool.clone();
            let quit = Arc::clone(&quit);
            std::thread::Builder::new()
                .name("媒体预览".to_string())
                .spawn(move || work(&pool, edge, &inbox, &outbox, &quit))
                .ok()
        };
        Self {
            pool,
            edge,
            program: FFMPEG.to_string(),
            jobs,
            done,
            cache: HashMap::new(),
            inflight: HashSet::new(),
            worker,
            quit,
        }
    }

    /// 换一个抽帧程序。**测试拿它走「ffmpeg 不在」那条路**。
    pub fn set_program(&mut self, program: impl Into<String>) {
        self.program = program.into();
    }

    /// 还有几件在后台跑着。界面拿它决定这一帧要不要再排一次重画。
    #[must_use]
    pub fn busy(&self) -> usize {
        self.inflight.len()
    }

    /// 把后台跑完的收进缓存，返回**要记进中立库的那些首帧**。
    ///
    /// 每帧开头调一次。返回空表示这一帧没有新东西落库——**不表示没有新图**，
    /// 新图已经进缓存了。
    pub fn drain(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        // **递活那一头断了也照收**：后台线程若已经走了，管子里剩下的那几件仍然算数。
        while let Ok(settled) = self.done.try_recv() {
            self.inflight.remove(&settled.key);
            if let Some(frame) = settled.frame {
                frames.push(frame);
            }
            self.cache.insert(settled.key, settled.preview);
        }
        frames
    }

    /// 要池里这一份的图。
    ///
    /// `frame` 是「这份视频抽过的首帧是哪一份」（中立库里 `media_frame` 那一行），
    /// 图片一律给 `None`。给得出首帧就直接解那一份，**这就是「第二次打开不重抽」**。
    ///
    /// 缓存里有就当场返回；没有就排上队、返回 `None`——那一帧界面画占位。
    pub fn want(&mut self, hash: &str, ext: &str, frame: Option<&str>) -> Option<&Preview> {
        let key: Key = (hash.to_string(), ext.to_string());
        if !self.cache.contains_key(&key) && self.inflight.insert(key.clone()) {
            // **记着抽过、可池里那个文件没了，就当没抽过。** 不这么判的话，人手动清过
            // 一次媒体池之后，那几段视频**永远**显示占位，而提示指着一个他从没见过的
            // 首帧哈希——那是个自己修不好的状态。
            let frame = frame.filter(|frame| self.pool.contains(frame, "png"));
            let job = if let Some(frame) = frame {
                // 抽过了：解那一份首帧，一个进程都不拉起。
                Job::Decode {
                    key: key.clone(),
                    hash: frame.to_string(),
                    ext: "png".to_string(),
                }
            } else if is_video(ext) {
                Job::Frame {
                    key: key.clone(),
                    hash: hash.to_string(),
                    ext: ext.to_string(),
                    program: self.program.clone(),
                }
            } else {
                Job::Decode {
                    key: key.clone(),
                    hash: hash.to_string(),
                    ext: ext.to_string(),
                }
            };
            if self.jobs.send(job).is_err() {
                // 后台线程没起来（或者已经走了）。**当场如实说**，别让这一格永远转圈。
                self.inflight.remove(&key);
                self.cache.insert(
                    key.clone(),
                    Preview::Missing(Missing::Unreadable {
                        why: "预览那条线程没起来".to_string(),
                    }),
                );
            }
        }
        self.cache.get(&key)
    }

    /// 这一份**眼下正在后台跑着**吗。
    ///
    /// 调用方拿它省掉白跑的活：一件排出去之后要等几百毫秒（抽帧那一档），这期间
    /// 每帧再去问一遍「这份视频的首帧记过没有」是一次白花的库查询——而那条查询
    /// 跑在画帧线程上，还要跟别处的写者抢锁。
    #[must_use]
    pub fn inflight(&self, hash: &str, ext: &str) -> bool {
        self.inflight.contains(&(hash.to_string(), ext.to_string()))
    }

    /// 缓存里眼下有几份。
    #[must_use]
    pub fn cached(&self) -> usize {
        self.cache.len()
    }

    /// 缓存**攒过头了就只留还看得见的那几份**。
    ///
    /// 不淘汰的话，按真库翻一遍是 440 张图加 178 段视频的首帧全解过一遍，
    /// 缓存里躺着六百来份 RGBA（单份最大 1 MiB），而且永不回落。
    /// **上限之内一份不动**：详情面板一次只摆几件，来回切两个变体不该每次重解。
    pub fn trim(&mut self, keep: &HashSet<Key>, limit: usize) {
        if self.cache.len() > limit {
            self.cache.retain(|key, _| keep.contains(key));
        }
    }

    /// **把后台跑完的等回来**，最多等这么久。
    ///
    /// 只给测试用：真界面上一帧一帧地 [`drain`](Self::drain) 就够了，
    /// 而 headless 测试里没有「下一帧」这回事，得有个地方等。
    /// 返回等回来的那些首帧。
    pub fn settle(&mut self, patience: std::time::Duration) -> Vec<Frame> {
        let deadline = std::time::Instant::now() + patience;
        let mut frames = Vec::new();
        while self.busy() > 0 && std::time::Instant::now() < deadline {
            frames.extend(self.drain());
            if self.busy() > 0 {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        frames.extend(self.drain());
        frames
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        // 两下告诉它收工：立一个旗子（它在每件活之前看两次），再把递活那一头换掉——
        // 旧的 `Sender` 就地析构，后台那条阻塞着的 `recv` 当场返回错误。
        self.quit.store(true, Ordering::Relaxed);
        let (dead, _) = channel();
        let _ = std::mem::replace(&mut self.jobs, dead);
        // **不等它。** 它手里那件活最坏是一个外部 ffmpeg 进程，等于把关窗口那一下
        // 挂在别人的进程上。撒手是安全的，靠的是三条构造上的事实：
        //
        // 1. 它**不碰中立库**——一个字节都不写（记库那一半在画帧线程上）。
        // 2. 它往池里落文件走的是「先写 tmp、算完 rename 进去」，`rename` 是原子的：
        //    半路被进程退出打断，池里不会多出半截文件，最多 `tmp/` 里留一个 `.part`
        //    ——那本来就是它的落脚处。
        // 3. 它交结果的那一头断了就自己收工（`outbox.send` 返回错误即 `return`）。
        drop(self.worker.take());
    }
}

/// 后台那条线程：一件一件做完，做完一件送一件。
fn work(
    pool: &MediaPool,
    edge: u32,
    inbox: &Receiver<Job>,
    outbox: &Sender<Settled>,
    quit: &AtomicBool,
) {
    // **先攒后做**：翻库时同一帧会一口气排进十几件，而人往往滚过去就不看了。
    // 攒一批再做，让「最后排进来的那几件」不至于排在一屏之外的那些后面。
    let mut queue: VecDeque<Job> = VecDeque::new();
    loop {
        if quit.load(Ordering::Relaxed) {
            return;
        }
        let job = match queue.pop_back() {
            Some(job) => job,
            None => match inbox.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
        };
        // 手里这件做之前，先把已经排进来的都收进队列——好让新的排在前头。
        while let Ok(more) = inbox.try_recv() {
            queue.push_back(more);
        }
        if quit.load(Ordering::Relaxed) {
            return;
        }
        let settled = run(pool, edge, job);
        if outbox.send(settled).is_err() {
            return;
        }
    }
}

/// 做一件。
fn run(pool: &MediaPool, edge: u32, job: Job) -> Settled {
    match job {
        Job::Decode { key, hash, ext } => Settled {
            preview: from_pool(pool, &hash, &ext, edge),
            key,
            frame: None,
        },
        Job::Frame {
            key,
            hash,
            ext,
            program,
        } => {
            let at = pool.path_of(&hash, &ext);
            if !at.is_file() {
                return Settled {
                    key,
                    preview: Preview::Missing(Missing::NotInPool { hash, ext }),
                    frame: None,
                };
            }
            match extract_frame(&program, &at) {
                Err(why) => Settled {
                    key,
                    preview: Preview::Missing(why),
                    frame: None,
                },
                Ok(png) => match store_frame(pool, &hash, &png) {
                    Err(source) => Settled {
                        key,
                        preview: Preview::Missing(Missing::Unreadable {
                            why: source.to_string(),
                        }),
                        frame: None,
                    },
                    Ok(frame) => Settled {
                        key,
                        preview: match decode(&png, edge) {
                            Ok(thumb) => Preview::Ready(Box::new(thumb)),
                            Err(why) => Preview::Missing(why),
                        },
                        frame: Some(frame),
                    },
                },
            }
        }
    }
}

/// 抽出来的首帧落进池里。**记库那一半不在这儿**——那要主线程那份写连接。
fn store_frame(pool: &MediaPool, video: &str, png: &[u8]) -> Result<Frame, PoolError> {
    let (hash, _) = pool.take_bytes(png, "png")?;
    Ok(Frame {
        video: video.to_string(),
        hash,
        bytes: u64::try_from(png.len()).unwrap_or(u64::MAX),
    })
}

/// 一个不可能存在的程序名，测「ffmpeg 不在」那条路用。
///
/// **不写在测试里**，是因为界面那一侧的 headless 测试也要走同一条路——
/// 两处各写一个名字，迟早有一处哪天真装上了同名的程序。
pub const NO_SUCH_PROGRAM: &str = "romcat-没有这个程序-ffmpeg";

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张纯色 PNG。
    fn png(width: u32, height: u32) -> Vec<u8> {
        let buf = image::RgbaImage::from_pixel(width, height, image::Rgba([9, 9, 9, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("编得出 PNG");
        out.into_inner()
    }

    #[test]
    fn 长边缩到上限而且纵横比一个像素都不歪() {
        let thumb = decode(&png(2000, 1000), 512).expect("解得开");
        assert_eq!(thumb.width(), 512);
        assert_eq!(thumb.height(), 256, "2:1 的图缩完还得是 2:1");
        assert_eq!(thumb.rgba().len(), 512 * 256 * 4);
    }

    #[test]
    fn 比上限小的图不放大() {
        // 一张 16×16 的 logo 拉到 512 只会糊成一片。
        let thumb = decode(&png(16, 16), 512).expect("解得开");
        assert_eq!((thumb.width(), thumb.height()), (16, 16));
    }

    #[test]
    fn 半截文件说解不开而不是崩() {
        let mut half = png(64, 64);
        half.truncate(20);
        let why = decode(&half, 512).expect_err("这不该解得开");
        assert!(matches!(why, Missing::Undecodable { .. }), "{why:?}");
    }

    #[test]
    fn ffmpeg不在时是一档退化不是一个错误() {
        let why = extract_frame(NO_SUCH_PROGRAM, Path::new("/不存在/片子.mp4"))
            .expect_err("没有这个程序");
        assert!(why.is_no_ffmpeg(), "{why:?}");
        assert!(why.render().contains("这台机器上没有"), "{}", why.render());
    }

    #[test]
    fn 认得出哪些解得开哪些当视频看() {
        assert!(decodable("JPEG"), "jpeg 折成 jpg 之后该解得开");
        assert!(decodable("png"));
        assert!(!decodable("mp4"));
        // gif 在池里收得下，可这一版不带它的解码器——那是两件事。
        assert!(!decodable("gif"));
        assert!(is_video("MP4"));
        assert!(!is_video("png"));
    }
}
