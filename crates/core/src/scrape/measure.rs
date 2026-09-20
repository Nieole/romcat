//! **媒体量尺**：一份媒体的**尺寸**（像素宽高）与**时长**（视频多长）。
//!
//! ## 为什么在入池那一刻量，而不是要看的时候量
//!
//! 作品详情页媒体那一格照设计稿写「来源 · 尺寸 · 时长 · 大小」，而那一页是一格接一格画的：
//! 真库的媒体池里 440 张图加 178 段视频（`docs/library-facts.md`），每画一帧现量一遍等于
//! 每次翻库解四百多张图、拉一百多个进程。**媒体池是内容寻址的**（ADR-0009），同一串字节
//! 量出来的数永远一样——所以这三样是**入池那一刻记一次**的账，与 `media.bytes` 同一档。
//!
//! 落点因此只有一处：[`pool::adopt_into`](super::pool)——主库那份边读边算、在线那份整块
//! 下回来，两条路前半截不同，**从收尾那一段起一模一样**。
//!
//! ## 量不出来不是错误
//!
//! 三样各有量不出来的时候：这一版不解的图片格式、半截文件、以及**这台机器上没有
//! ffmpeg**。三者的产出都是「那一格留空」，屏上那一行退回「来源 · 大小」——与
//! [`preview`](super::preview) 那边「ffmpeg 不在就退化成占位图标」是同一条纪律
//! （ADR-0021：**不可读**是第三态，不是零）。**老库里此前已经入过池的那些三样都空着**，
//! 屏上照样退回「来源 · 大小」：为这三个数逼人重扫一份 8.60 TiB 的库换不到任何东西。
//!
//! ## 视频走的是**外部进程**，与抽首帧同一个程序
//!
//! 理由与 [`preview::extract_frame`](super::preview::extract_frame) 同源：ffmpeg 的 Rust
//! 绑定依赖涨一大截、Windows 上构建麻烦，而 Windows 正是主力机（ADR-0018）。这里连
//! **第二个程序都不引**——不叫 `ffprobe`，就是 `ffmpeg -i`，因为那样「它在不在」只有
//! 一个答案；引进第二个程序等于给「这台机器能不能量视频」造出第二个判据。
//!
//! 代价是**要读它写给人看的那几行**（[`read_probe`]）。这一笔明摆着：`Duration:` 与视频
//! 流那一行的 `宽x高` 是 ffmpeg 十几年没变过的两处，而解析那一段是**纯函数**、直接钉了
//! 测试——真出错时红的是那条测试，不是某台机器上的某一份媒体。

use std::path::Path;

/// 一份媒体量出来的那几样。**量不出来的那一格是 `None`，不是 0**（ADR-0021）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Measured {
    /// 像素宽；图片与视频都有，量不出来是 `None`。
    pub width: Option<u32>,
    /// 像素高。
    pub height: Option<u32>,
    /// 时长（毫秒）；只有视频有，没有 ffmpeg 时是 `None`。
    pub duration_ms: Option<u64>,
}

impl Measured {
    /// 三样一个都没量出来吗。**库里那三列全空时读回来就是它**，于是「老库里入过池的
    /// 那些」与「这一份量不出来」在读的那一侧是同一件事——两者对屏上那一行的处置本来
    /// 就一样。
    #[must_use]
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// 宽高都有吗；有就是 `(宽, 高)`。
    #[must_use]
    pub fn size(self) -> Option<(u32, u32)> {
        self.width.zip(self.height)
    }
}

/// 量一份**已经落在盘上**的媒体。
///
/// `program` 是量视频那个外部程序（[`preview::FFMPEG`](super::preview::FFMPEG)）。
/// **单独收进来是为了让「它不在」那条路测得到**——验收要的正是那条退化路走得通，
/// 而不是「跑测试这台机器碰巧没装」（同 [`extract_frame`](super::preview::extract_frame)
/// 的那个参数）。
///
/// 认不出是图也不是视频的（`normalized_ext` 收得下、但既不解码也不是视频的那些）
/// 交回空的一份。
#[must_use]
pub fn measure(at: &Path, ext: &str, program: &str) -> Measured {
    if super::preview::decodable(ext) {
        return image_size(at);
    }
    if super::preview::is_video(ext) {
        return probe_video(program, at);
    }
    Measured::default()
}

/// 一张图的宽高，**只读文件头**。
///
/// 不解整张：这一趟跑在入池那条路上，而池里那些封面动辄四千像素——解开只为了读两个数，
/// 峰值内存与耗时都白花。`image` 的 `into_dimensions` 读到头就停。
fn image_size(at: &Path) -> Measured {
    let Ok(reader) = image::ImageReader::open(at) else {
        return Measured::default();
    };
    let Ok(reader) = reader.with_guessed_format() else {
        return Measured::default();
    };
    match reader.into_dimensions() {
        Ok((width, height)) => Measured {
            width: Some(width),
            height: Some(height),
            duration_ms: None,
        },
        // **解不开不是错误**：半截文件、名字与内容对不上，那一格留空就是了。
        Err(_) => Measured::default(),
    }
}

/// 问 ffmpeg 要一份视频的宽高与时长。
///
/// `ffmpeg -i <文件>` **没有输出文件，所以它一定以非零码退出**（「At least one output
/// file must be specified」）——那不是失败，要的那几行已经写在 stderr 上了。因此这里
/// **不看退出码**，只读它说了什么。
fn probe_video(program: &str, at: &Path) -> Measured {
    let out = std::process::Command::new(program)
        // `-nostdin` 防它把标准输入抢走（同 `preview::extract_frame`）。
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-i")
        .arg(at)
        .output();
    // **程序不在、跑不起来都只是「量不出来」**：这台机器上没有 ffmpeg 是允许的退化。
    let Ok(out) = out else {
        return Measured::default();
    };
    read_probe(&String::from_utf8_lossy(&out.stderr))
}

/// 从 ffmpeg 写给人看的那几行里读出宽高与时长。**纯函数**，钉得住测试。
///
/// 读两处：
/// - `Duration: 00:00:30.00, start: …` —— `N/A` 是「它自己也不知道」，留空。
/// - 头一条 `Stream #0:0…: Video: …, 640x480 [SAR 1:1 DAR 4:3], …` 里那个 `宽x高`。
#[must_use]
pub fn read_probe(text: &str) -> Measured {
    let mut out = Measured::default();
    for line in text.lines() {
        let trimmed = line.trim();
        if out.duration_ms.is_none()
            && let Some(rest) = trimmed.strip_prefix("Duration:")
        {
            out.duration_ms = read_clock(rest.split(',').next().unwrap_or("").trim());
        }
        if out.width.is_none()
            && trimmed.starts_with("Stream #")
            && let Some((_, tail)) = trimmed.split_once(": Video:")
            && let Some((width, height)) = read_frame_size(tail)
        {
            out.width = Some(width);
            out.height = Some(height);
        }
    }
    out
}

/// `HH:MM:SS.ff` 读成毫秒；`N/A` 与读不成的都是 `None`。
fn read_clock(text: &str) -> Option<u64> {
    let (clock, frac) = text.split_once('.').unwrap_or((text, "0"));
    let mut parts = clock.split(':');
    let hours: u64 = parts.next()?.trim().parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || minutes >= 60 || seconds >= 60 {
        return None;
    }
    // 小数点后**只认前两位**（ffmpeg 写的是百分之一秒）；多了少了都按两位补齐。
    let hundredths: u64 = format!("{frac:0<2}").get(..2)?.parse().ok()?;
    Some(((hours * 3600 + minutes * 60 + seconds) * 100 + hundredths) * 10)
}

/// 视频流那一行里那个 `宽x高`。
///
/// **按逗号分段、每段只看头一个词**，而且两边都得是 2–5 位、不带前导零的数字。
/// 这两道闸一起挡的是同一样东西：那一行里 `(avc1 / 0x31637661)` 这种四字码，
/// 松一点就会被读成 `0 × 31637661`，而那个数会一路画到屏上去。
fn read_frame_size(tail: &str) -> Option<(u32, u32)> {
    tail.split(',')
        .filter_map(|field| field.split_whitespace().next())
        .find_map(|token| {
            let (width, height) = token.split_once('x')?;
            Some((read_side(width)?, read_side(height)?))
        })
}

/// `宽x高` 的一边：2–5 位数字，不带前导零。
fn read_side(text: &str) -> Option<u32> {
    if !(2..=5).contains(&text.len())
        || !text.bytes().all(|byte| byte.is_ascii_digit())
        || text.starts_with('0')
    {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ffmpeg 5.1 对着一段 640×480 的 mp4 吐出来的那几行，原样抄的。
    const 一段真的输出: &str = "\
Input #0, mov,mp4,m4a,3gp,3g2,mj2, from '/池/ab/abc.mp4':
  Metadata:
    major_brand     : isom
  Duration: 00:00:30.04, start: 0.000000, bitrate: 3276 kb/s
  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 640x480 [SAR 1:1 DAR 4:3], 3276 kb/s, 30 fps, 30 tbr, 15360 tbn (default)
  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, stereo, fltp, 128 kb/s (default)
At least one output file must be specified";

    #[test]
    fn ffmpeg写给人看的那几行里读得出宽高与时长() {
        let got = read_probe(一段真的输出);
        assert_eq!(got.size(), Some((640, 480)));
        assert_eq!(got.duration_ms, Some(30_040));
    }

    #[test]
    fn 视频流那一行里的四字码不许被读成宽高() {
        // `(avc1 / 0x31637661)` 松一点就会被读成 `0 × 31637661`——那个数会一路画到屏上。
        let got =
            read_probe("  Stream #0:0: Video: h264 (avc1 / 0x31637661), yuv420p, 320x240, 30 fps");
        assert_eq!(got.size(), Some((320, 240)));
    }

    #[test]
    fn 时长那一格说不知道时留空而不是零() {
        let got = read_probe("  Duration: N/A, start: 0.000000, bitrate: N/A");
        assert_eq!(got.duration_ms, None, "「它自己也不知道」不是 0");
        assert!(got.is_empty());
    }

    #[test]
    fn 整点与带小数的时长都读得准() {
        assert_eq!(read_clock("00:00:30.00"), Some(30_000));
        assert_eq!(read_clock("01:02:03.45"), Some(3_723_450));
        assert_eq!(read_clock("00:00:00.50"), Some(500));
        assert_eq!(read_clock("N/A"), None);
        assert_eq!(read_clock("00:99:00.00"), None, "分钟不该超过 59");
    }

    #[test]
    fn 量不出来的时候交回空的一份而不是报错() {
        // 认不出是图也不是视频的：一格都不量。
        assert!(measure(Path::new("/不存在/x.txt"), "txt", "ffmpeg").is_empty());
        // 图片但文件压根不在。
        assert!(measure(Path::new("/不存在/封面.png"), "png", "ffmpeg").is_empty());
        // 视频，而这台机器上没有那个程序——**这是允许的退化，不是错误**。
        assert!(
            measure(
                Path::new("/不存在/片子.mp4"),
                "mp4",
                super::super::preview::NO_SUCH_PROGRAM,
            )
            .is_empty()
        );
    }
}
