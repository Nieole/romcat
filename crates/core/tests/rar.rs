//! 穿透 rar：**自己写的头部解析器**、分卷、以及三个会一声不响压低命中率的坑。
//!
//! 票 04 守的东西与票 03 不同。zip 与 7z 的难点在「读得到吗」，rar 的难点在
//! **「读到的那个数是不是它看起来的意思」**：
//!
//! - 分卷的**非末段**，校验和记的是**打包后数据**的（ADR-0014）；
//! - 带密码时校验和被密钥搅过（`unrar lt` 印成 `CRC32 MAC`）；
//! - 只写 BLAKE2sp 的 RAR5 压根没有 CRC-32，而 DAT 一条 BLAKE2 都不记。
//!
//! 三种都不会报错，只会**撞不上**，然后被记成「未命中」。所以每一种都得有一条
//! 测试盯着「这一栏必须是空的」。

use romcat_core::container::volume::{self, VolumeScheme};
use romcat_core::container::{self, ContainerError, ContainerKind, ReadPlan};
use romcat_core::fs::MemFs;
use romcat_core::testing::container::{
    RarEntrySpec, crc32, rar4_archive, rar4_volume, rar5_archive, rar5_volume,
};

fn 样本(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| seed.wrapping_add((i % 251) as u8).wrapping_mul(31))
        .collect()
}

fn 建库(name: &str, bytes: Vec<u8>) -> MemFs {
    let mut library = MemFs::new();
    library.file(name, bytes);
    library
}

fn 穿(library: &MemFs, name: &str) -> container::Listing {
    container::list(library, std::path::Path::new(name)).expect("穿得透")
}

#[test]
fn rar5_不解压就读出内部文件的名字与未压缩大小与_crc32() {
    let 甲 = 样本(2, 1000);
    let 乙 = 样本(4, 2000);
    let library = 建库(
        "/lib/FC/合集.rar",
        rar5_archive(&[
            RarEntrySpec::stored("超级马里奥.nes", 甲.clone()),
            RarEntrySpec::compressed("魂斗罗.nes", vec![0xAA; 300]).with_size(乙.len() as u64),
            RarEntrySpec::dir("说明"),
        ]),
    );
    let listing = 穿(&library, "/lib/FC/合集.rar");
    assert_eq!(listing.kind, ContainerKind::Rar);
    let entries = &listing.contents.entries;
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].path, "超级马里奥.nes");
    assert_eq!(entries[0].size, 1000);
    assert_eq!(entries[0].crc32, Some(crc32(&甲)));
    assert_eq!(entries[1].size, 2000, "未压缩大小，不是打包后的长度");
    assert!(entries[2].is_dir);
    assert_eq!(entries[2].block, None, "目录不占块");
    // 服务头（quick open 的 `QO`）不是内部文件
    assert!(!entries.iter().any(|e| e.path == "QO"));
    assert!(!listing.contents.needs_full_decompress());
}

#[test]
fn rar4_不解压就读出内部文件的名字与未压缩大小与_crc32() {
    let 甲 = 样本(7, 512);
    let library = 建库(
        "/lib/SFC/合集.rar",
        rar4_archive(&[
            RarEntrySpec::stored("塞尔达.sfc", 甲.clone()),
            RarEntrySpec::dir("补丁"),
        ]),
    );
    let listing = 穿(&library, "/lib/SFC/合集.rar");
    let entries = &listing.contents.entries;
    assert_eq!(entries[0].path, "塞尔达.sfc");
    assert_eq!(entries[0].size, 512);
    // **RAR4 的 CRC-32 无条件存在**：格式里没有「这一条没记 CRC」这回事
    assert_eq!(entries[0].crc32, Some(crc32(&甲)));
    assert!(entries[1].is_dir);
}

#[test]
fn rar4_的私有_unicode_名字解得出中文() {
    // 非 Unicode 那一半是 GBK 的 `A`，Unicode 那一半照着它抄一个字节再补一个汉字。
    // 真机上这条路验过：随机 30 份带中文名的 RAR4 逐条与 `unrar lb` 对比一字不差。
    let encoded = [0x59u8, 0b0001_0000, b'A', 0x29];
    let library = 建库(
        "/lib/FC/名字.rar",
        rar4_archive(&[RarEntrySpec::stored_raw(b"A", 样本(1, 16)).with_unicode_name(&encoded)]),
    );
    let listing = 穿(&library, "/lib/FC/名字.rar");
    assert_eq!(listing.contents.entries[0].path, "A\u{5929}");
    assert!(!listing.contents.entries[0].name_lossy);
}

#[test]
fn 分卷的多个分卷是一个容器而不是一堆孤立碎片() {
    let 整份 = 样本(9, 3000);
    let mut library = MemFs::new();
    library.file(
        "/lib/PS2/大作.part1.rar",
        rar5_volume(
            &[RarEntrySpec::compressed("大作.iso", vec![1u8; 100])
                .split_after(0xDEAD_BEEF, 整份.len() as u64)],
            0,
            true,
        ),
    );
    library.file(
        "/lib/PS2/大作.part2.rar",
        rar5_volume(
            &[RarEntrySpec::compressed("大作.iso", vec![2u8; 80]).split_before(整份.len() as u64)],
            1,
            false,
        ),
    );

    // 入口卷一次交出整组的内部构成
    let listing = 穿(&library, "/lib/PS2/大作.part1.rar");
    assert_eq!(listing.contents.entries.len(), 1, "两卷合起来是一个条目");
    assert_eq!(listing.contents.entries[0].path, "大作.iso");
    assert_eq!(listing.contents.entries[0].size, 3000);

    // **非入口段不是一个独立的容器**：扫描不会单独去穿它，报告也不会把一组数成两个
    assert_eq!(
        ContainerKind::for_path(std::path::Path::new("/lib/PS2/大作.part2.rar")),
        None
    );
    assert_eq!(
        ContainerKind::for_path(std::path::Path::new("/lib/PS2/大作.part1.rar")),
        Some(ContainerKind::Rar)
    );
}

#[test]
fn 分卷非末段的校验和是打包后数据的_不许拿去撞_dat() {
    // ⭐ 这是这张票最要紧的一条。technote 原文：*For files split between volumes it
    // contains CRC32 of file packed data … for all file parts except the last.*
    // 真机实测一份三卷的 RAR5：同一个文件三卷分别记 DDFAE192 / B31EA58F / 9506C014，
    // 只有末段那个才是未压缩数据的 CRC——`unrar lt` 把前两个印成 `Pack-CRC32`。
    let 末段真值 = 0x1234_5678u32;
    let mut library = MemFs::new();
    library.file(
        "/lib/PS2/半截.part1.rar",
        rar5_volume(
            &[RarEntrySpec::compressed("大作.iso", vec![1u8; 64]).split_after(0xDEAD_BEEF, 9000)],
            0,
            true,
        ),
    );
    library.file(
        "/lib/PS2/半截.part2.rar",
        rar5_volume(
            &[RarEntrySpec::compressed("大作.iso", vec![2u8; 64])
                .split_before(9000)
                .with_crc32(末段真值)],
            1,
            false,
        ),
    );
    let listing = 穿(&library, "/lib/PS2/半截.part1.rar");
    assert_eq!(
        listing.contents.entries[0].crc32,
        Some(末段真值),
        "**末段的那个才是未压缩数据的 CRC**，前面几卷的一律不要"
    );
    assert_ne!(listing.contents.entries[0].crc32, Some(0xDEAD_BEEF));
}

#[test]
fn 缺一卷时不交半张清单() {
    // 缺卷时跨卷条目的校验和还停在「打包后数据」那一档，而报告里「读出来了」的含义
    // 是「这就是里面的全部」。如实说缺了哪一卷，人才知道下一步是去找那一卷。
    let library = 建库(
        "/lib/PS2/缺卷.part1.rar",
        rar5_volume(
            &[RarEntrySpec::compressed("大作.iso", vec![1u8; 64]).split_after(0xDEAD_BEEF, 9000)],
            0,
            true,
        ),
    );
    let error = container::list(&library, std::path::Path::new("/lib/PS2/缺卷.part1.rar"))
        .expect_err("缺一卷就读不全");
    assert!(
        matches!(error, ContainerError::MissingVolume { .. }),
        "得到的是 {error}"
    );
    assert_eq!(
        error.reason(),
        container::FailureReason::MissingVolume,
        "「缺分卷」与「结构读不下去」的下一步完全不同"
    );
    assert!(error.to_string().contains("缺卷.part2.rar"), "{error}");
}

#[test]
fn 只写_blake2sp_的容器被标记为要完整解压() {
    // `rar a -htb` 的产物：file flag 的 0x0004 位是 0，哈希搬进 extra area 的 0x02
    // 记录。而 No-Intro / Redump / TOSEC **一条 BLAKE2 都不记**——这种容器进不了
    // 零解压的第一命中层，与 zst 同一档（ADR-0014）。
    let library = 建库(
        "/lib/MD/强校验.rar",
        rar5_archive(&[
            RarEntrySpec::stored("魂斗罗.md", 样本(3, 800)).blake2_only(),
            RarEntrySpec::dir("目录"),
        ]),
    );
    let listing = 穿(&library, "/lib/MD/强校验.rar");
    assert_eq!(listing.contents.entries[0].crc32, None);
    assert_eq!(listing.contents.without_crc(), 1);
    assert!(
        listing.contents.needs_full_decompress(),
        "有内容的条目一条 CRC-32 都没有"
    );
    // **穿透归穿透**：名字与大小照样读得出来，只是第一命中层用不上它
    assert_eq!(listing.contents.entries[0].path, "魂斗罗.md");
    assert_eq!(listing.contents.entries[0].size, 800);
}

#[test]
fn 带密码时被搅过的校验和一律丢掉() {
    // 真机实测：随机 60 份 rar 里 **20 份**带密码。technote 原文
    // *Use tweaked checksums … all file checksums are modified according to encryption
    // key value*，`unrar lt` 把那一栏印成 `CRC32 MAC`。不丢掉它，这一批会拿着一串
    // 谁也对不上的数去撞 DAT，然后被记成「未命中」——一声不响地把命中率压低。
    let library = 建库(
        "/lib/FC/带密码.rar",
        rar5_archive(&[
            RarEntrySpec::stored("坦克大战.nes", 样本(5, 400)).tweaked_checksum(0xD3A6_2879)
        ]),
    );
    let listing = 穿(&library, "/lib/FC/带密码.rar");
    assert_eq!(
        listing.contents.entries[0].crc32, None,
        "MAC 不是那份数据的 CRC-32"
    );
    assert_eq!(
        listing.contents.entries[0].size, 400,
        "名字与大小仍然是真的"
    );
}

#[test]
fn solid_的容器认得出来而且说得出代价() {
    // RAR 的 solid 比 7z 更糟：头部**没有**等价于 `NumUnPackStreamsInFolders` 的块
    // 映射，一条 solid 流只能从头顺序解到底（ADR-0014）。认得出来，报告才说得出
    // 「这个容器解起来贵」。
    let library = 建库(
        "/lib/SFC/solid.rar",
        rar5_archive(&[
            RarEntrySpec::compressed("甲.sfc", vec![1u8; 50]).with_size(1000),
            RarEntrySpec::compressed("乙.sfc", vec![2u8; 50])
                .with_size(1000)
                .solid(),
            RarEntrySpec::compressed("丙.sfc", vec![3u8; 50])
                .with_size(1000)
                .solid(),
        ]),
    );
    let listing = 穿(&library, "/lib/SFC/solid.rar");
    assert_eq!(listing.contents.blocks, 1, "三条挤在一个 solid 流里");
    assert!(listing.contents.is_solid());
    assert_eq!(listing.contents.entries_in_block(0), 3);
}

#[test]
fn 原样存放的条目读得出字节而压缩过的如实报解不了() {
    // 解压那条路要 UnRAR 的算法，而这个项目刻意没有引进它（许可传染，ADR-0014）。
    // **能交的先交出去**：识别那一侧对每一条各记各的账，一条读不了不该把已经读到的
    // 那些也一起废掉。
    let 甲 = 样本(11, 300);
    let library = 建库(
        "/lib/GB/混.rar",
        rar5_archive(&[
            RarEntrySpec::stored("能读.gb", 甲.clone()),
            RarEntrySpec::compressed("读不了.gb", vec![9u8; 40]).with_size(500),
        ]),
    );
    let listing = 穿(&library, "/lib/GB/混.rar");
    let plan = ReadPlan::all(&listing);
    let mut 读到: Vec<(String, Vec<u8>)> = Vec::new();
    let outcome = container::read_entries(
        &library,
        std::path::Path::new("/lib/GB/混.rar"),
        &listing,
        &plan,
        &mut |entry, reader| {
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer)?;
            读到.push((entry.path.clone(), buffer));
            Ok(())
        },
    );
    assert_eq!(读到.len(), 1, "原样存放的那一条交出来了");
    assert_eq!(读到[0].0, "能读.gb");
    assert_eq!(读到[0].1, 甲);
    let error = outcome.expect_err("压缩过的那一条要如实报解不了");
    assert!(
        matches!(error, ContainerError::UnsupportedMethod(_)),
        "得到的是 {error}"
    );
    assert!(error.to_string().contains("UnRAR"), "{error}");
}

#[test]
fn 跨卷的原样存放条目接起来就是原文() {
    // 跨卷的文件在磁盘上就是几段字节，中间隔着下一卷的签名与头部。原样存放时
    // 接起来就是原文——这也是这一层唯一读得了的情形。
    let 整份 = 样本(13, 200);
    let (头, 尾) = 整份.split_at(120);
    let mut library = MemFs::new();
    library.file(
        "/lib/GBA/跨卷.part1.rar",
        rar5_volume(
            &[RarEntrySpec::stored("游戏.gba", 头.to_vec())
                .split_after(0xAAAA_BBBB, 整份.len() as u64)],
            0,
            true,
        ),
    );
    library.file(
        "/lib/GBA/跨卷.part2.rar",
        rar5_volume(
            &[RarEntrySpec::stored("游戏.gba", 尾.to_vec())
                .split_before(整份.len() as u64)
                .with_crc32(crc32(&整份))],
            1,
            false,
        ),
    );
    let listing = 穿(&library, "/lib/GBA/跨卷.part1.rar");
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&整份)));
    let plan = ReadPlan::all(&listing);
    let mut 读到 = Vec::new();
    container::read_entries(
        &library,
        std::path::Path::new("/lib/GBA/跨卷.part1.rar"),
        &listing,
        &plan,
        &mut |_, reader| reader.read_to_end(&mut 读到).map(|_| ()),
    )
    .expect("两段接得起来");
    assert_eq!(读到, 整份, "接起来逐字节等于原文");
}

#[test]
fn 只要前几百字节时代价与条目总大小无关() {
    let 甲 = 样本(17, 100_000);
    let library = 建库(
        "/lib/NDS/大.rar",
        rar5_archive(&[RarEntrySpec::stored("大.nds", 甲.clone())]),
    );
    let listing = 穿(&library, "/lib/NDS/大.rar");
    let plan = ReadPlan::prefix(&listing, 512);
    let mut 读到 = Vec::new();
    let stats = container::read_entries(
        &library,
        std::path::Path::new("/lib/NDS/大.rar"),
        &listing,
        &plan,
        &mut |_, reader| reader.read_to_end(&mut 读到).map(|_| ()),
    )
    .expect("读得了");
    assert_eq!(读到.len(), 512);
    assert_eq!(读到, 甲[..512]);
    assert_eq!(stats.bytes_decompressed, 512, "读够就停，不把 100 KB 走完");
}

#[test]
fn 旧式分卷从那个普通的_rar_走到_r00() {
    // RAR4 的旧式编号：入口是 `X.rar`，第二卷是 `X.r00`（unrar `NextVolumeName()`）。
    let mut library = MemFs::new();
    library.file(
        "/lib/PS/老包.rar",
        rar4_volume(
            &[RarEntrySpec::compressed("光盘.bin", vec![1u8; 32]).split_after(0xCAFE, 5000)],
            true,
            false,
        ),
    );
    library.file(
        "/lib/PS/老包.r00",
        rar4_volume(
            &[RarEntrySpec::compressed("光盘.bin", vec![2u8; 32])
                .split_before(5000)
                .with_crc32(0x5555_6666)],
            false,
            false,
        ),
    );
    let listing = 穿(&library, "/lib/PS/老包.rar");
    assert_eq!(listing.contents.entries.len(), 1);
    assert_eq!(listing.contents.entries[0].size, 5000);
    assert_eq!(listing.contents.entries[0].crc32, Some(0x5555_6666));
    assert!(volume::is_non_entry_part("老包.r00"));
}

#[test]
fn 三种分卷形态是三样不同的东西() {
    // 认混了每一种都会处理错：RAR 的数据跨卷续接、`.001` 是纯字节切分（`cat` 即可
    // 还原）、ZIP split 的**入口是最后那个 `.zip`**（APPNOTE §8.3.4：末段用 `.zip`
    // 扩展名正是为了快速读到中央目录）。
    assert_eq!(
        volume::volume_of("游戏.part1.rar").map(|v| v.scheme),
        Some(VolumeScheme::Rar)
    );
    assert_eq!(
        volume::volume_of("合集.7z.001").map(|v| v.scheme),
        Some(VolumeScheme::ByteSplit)
    );
    assert_eq!(
        volume::volume_of("包.z01").map(|v| v.scheme),
        Some(VolumeScheme::ZipSplit)
    );
    let 组 = volume::group_volumes(["包.z01", "包.z02", "包.zip"]);
    assert_eq!(组[0].entry.as_deref(), Some("包.zip"), "入口是末段");
}

#[test]
fn 扩展名是_rar_而字节不是() {
    let library = 建库("/lib/假的.rar", vec![0u8; 64]);
    let error = container::list(&library, std::path::Path::new("/lib/假的.rar")).unwrap_err();
    assert!(
        matches!(error, ContainerError::NotAContainer { .. }),
        "得到的是 {error}"
    );
}

#[test]
fn 头部加密的容器说得出是要密码() {
    // RAR4 的 `MHD_PASSWORD`：头部加密时连内部文件名都看不到。与「结构读不下去」
    // 分开——那个是该去看看这文件到底是什么，这个是等人给密码。
    let mut bytes = rar4_archive(&[RarEntrySpec::stored("甲.nes", 样本(1, 10))]);
    // 主头部的旗标在签名之后第 3、4 字节上。
    bytes[7 + 3] |= 0x80;
    let library = 建库("/lib/加密.rar", bytes);
    let error = container::list(&library, std::path::Path::new("/lib/加密.rar")).unwrap_err();
    assert!(
        matches!(error, ContainerError::Encrypted),
        "得到的是 {error}"
    );
    assert_eq!(error.reason(), container::FailureReason::NeedsPassword);
}

#[test]
fn 一趟只要一条时别的条目一个字节都不读() {
    let library = 建库(
        "/lib/FC/两个.rar",
        rar5_archive(&[
            RarEntrySpec::stored("甲.nes", 样本(1, 5000)),
            RarEntrySpec::stored("乙.nes", 样本(2, 5000)),
        ]),
    );
    let listing = 穿(&library, "/lib/FC/两个.rar");
    let plan = ReadPlan::only(&listing, 1);
    assert_eq!(plan.blocks_needed(&listing), 1);
    let mut 名字 = Vec::new();
    let stats = container::read_entries(
        &library,
        std::path::Path::new("/lib/FC/两个.rar"),
        &listing,
        &plan,
        &mut |entry, reader| {
            名字.push(entry.path.clone());
            std::io::copy(reader, &mut std::io::sink()).map(|_| ())
        },
    )
    .expect("读得了");
    assert_eq!(名字, ["乙.nes"]);
    assert_eq!(stats.entries_read, 1);
    assert_eq!(stats.bytes_decompressed, 5000);
}
