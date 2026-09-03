//! 造几份 **Switch 容器**的样本字节：PFS0（`.nsp` / `.nsz`）与 XCI（`.xci` / `.xcz`）。
//!
//! **只造容器层，不造内容**。这正是票 27 那条纪律的形状：容器的头、条目表与字符串表
//! 全是明文，识别读的就是这些；而里面那些 NCA 是密文，这一层压根不碰——所以样本里
//! 那些「内容」是几个零字节，一个字节的真 NCA 都不需要，也**绝不需要任何密钥**。
//!
//! 条目名照真机抄：真库里那 146 个文件的文件名表实测只有两种形状，
//! `<32 位 hex>.nca` / `.cnmt.nca` / `.ncz` 与 `<32 位 hex>.tik` / `.cert`
//! （`docs/research/switch-identification.md` §2.2.3）。

/// PFS0 一条条目多大。
const PFS0_ENTRY: usize = 0x18;

/// HFS0 一条条目多大（多了被哈希区域的大小与一段 SHA-256）。
const HFS0_ENTRY: usize = 0x40;

/// 造一张 PFS0：一个裸 `.nsp` / `.nsz` 就是它本身。
///
/// 每一条给 16 字节的零当「内容」——识别一个字节都不看它们。
#[must_use]
pub fn pfs0(names: &[&str]) -> Vec<u8> {
    partition(names, PFS0_ENTRY, b"PFS0", &[])
}

/// 造一张 HFS0。`offsets` 给了就用它当每一条的数据偏移（相对数据区起点），
/// 空着就按顺序摆。**只服务 [`xci`]**——一张裸 HFS0 在真库里不存在，
/// 对外公开它就是造一个没人用得上的形状。
fn hfs0(names: &[&str], offsets: &[u64]) -> Vec<u8> {
    partition(names, HFS0_ENTRY, b"HFS0", offsets)
}

fn partition(names: &[&str], entry_size: usize, magic: &[u8; 4], offsets: &[u64]) -> Vec<u8> {
    let mut strings: Vec<u8> = Vec::new();
    let mut at: Vec<u32> = Vec::new();
    for name in names {
        at.push(u32::try_from(strings.len()).unwrap_or(0));
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
    }
    // 字符串表补齐到 16 的倍数——真机上的转储都是这么对齐的。
    while !strings.len().is_multiple_of(0x10) {
        strings.push(0);
    }
    let count = u32::try_from(names.len()).unwrap_or(0);
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(magic);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&u32::try_from(strings.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    for (index, string_at) in at.iter().enumerate() {
        let mut entry = vec![0_u8; entry_size];
        let offset = offsets
            .get(index)
            .copied()
            .unwrap_or_else(|| u64::try_from(index).unwrap_or(0) * 16);
        entry[0..8].copy_from_slice(&offset.to_le_bytes());
        entry[8..16].copy_from_slice(&16_u64.to_le_bytes());
        entry[16..20].copy_from_slice(&string_at.to_le_bytes());
        out.extend_from_slice(&entry);
    }
    out.extend_from_slice(&strings);
    out.extend_from_slice(&vec![0_u8; names.len() * 16]);
    out
}

/// 造一份卡带转储。
///
/// `base` 是 CardHeader 在文件里的偏移：**scene 风格的转储是 `0`，FullXCI 是 `0x1000`**
/// ——那 `0x1000` 字节的 CardKeyArea 写完就再也读不出来，所以两种形态都真实存在
/// （调研 §2.1.1，四个独立来源交叉验证）。
///
/// `partitions` 是根 HFS0 里那几个分区的名字，`secure` 是给 `secure` 分区的条目名。
/// **`secure` 分区被有意摆到远处**（几百 KB 之后），照真机的样子：那里前面还压着整套
/// 系统更新，真机实测落在文件的 368 MB 处——**前缀读一辈子也够不着，只能 seek**。
#[must_use]
pub fn xci(base: usize, partitions: &[&str], secure: &[&str]) -> Vec<u8> {
    /// 根 HFS0 摆在哪（相对 CardHeader 起点）。
    const ROOT: u64 = 0xF000;
    /// `secure` 分区摆在哪（相对 CardHeader 起点）。
    const SECURE: u64 = 0x4_0000;

    let mut out = vec![0_u8; base + usize::try_from(SECURE).unwrap_or(0) + 0x1000];
    out[base + 0x100..base + 0x104].copy_from_slice(b"HEAD");
    // 卡容量码：`0xE0` = 8GB。明文，识别用不着，摆着是为了让样本像真的。
    out[base + 0x10D] = 0xE0;
    out[base + 0x130..base + 0x138].copy_from_slice(&ROOT.to_le_bytes());

    let root_at = u64::try_from(base).unwrap_or(0) + ROOT;
    // 根 HFS0 的数据区起点：头 0x10 + 条目表 + 字符串表。
    let strings = padded_string_table(partitions);
    let data = root_at
        + 0x10
        + u64::try_from(partitions.len() * HFS0_ENTRY).unwrap_or(0)
        + u64::try_from(strings).unwrap_or(0);
    let secure_at = u64::try_from(base).unwrap_or(0) + SECURE;
    let offsets: Vec<u64> = partitions
        .iter()
        .map(|name| {
            if *name == "secure" {
                secure_at.saturating_sub(data)
            } else {
                0
            }
        })
        .collect();
    let root = hfs0(partitions, &offsets);
    let at = usize::try_from(root_at).unwrap_or(0);
    out[at..at + root.len()].copy_from_slice(&root);

    let table = hfs0(secure, &[]);
    let at = usize::try_from(secure_at).unwrap_or(0);
    out[at..at + table.len()].copy_from_slice(&table);
    out
}

/// 一张表的字符串表补齐之后有多长。
fn padded_string_table(names: &[&str]) -> usize {
    let mut len: usize = names.iter().map(|name| name.len() + 1).sum();
    while !len.is_multiple_of(0x10) {
        len += 1;
    }
    len
}
