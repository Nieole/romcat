# 游戏元数据与媒体资源在线刮削数据源调研

> **调研日期**：2026-08-30
> **方法**：仅依据一手来源 —— 各数据源官方 API 文档、官方注册页/条款页、开源刮削器源码（Skyscraper、ES-DE）、官方 GitHub 仓库与 LICENSE 文件；对 Bangumi / VNDB 另做了**实时 API 调用**取得直接证据。
> **标注约定**：
> - **【事实】** = 可在所引 URL 中直接读到的原文/可复现的实测结果
> - **【推断】** = 基于事实的合理推论，未被任何一手来源明文陈述
> - **【未确认】** = 未能从一手来源确认

---

## 目录

0. [执行摘要（TL;DR）](#执行摘要tldr)
1. [ScreenScraper.fr](#1-screenscraperfr)
2. [TheGamesDB (TGDB)](#2-thegamesdb-tgdb)
3. [IGDB / Twitch API](#3-igdb--twitch-api)
4. [LaunchBox Games Database](#4-launchbox-games-database)
5. [libretro-database / RetroArch / libretro-thumbnails](#5-libretro-database--retroarch--libretro-thumbnails)
6. [MobyGames API](#6-mobygames-api)
7. [GiantBomb API](#7-giantbomb-api)
8. [No-Intro / Redump / TOSEC / MAME hash](#8-no-intro--redump--tosec--mame-hash)
9. [中文/华语区数据源](#9-中文华语区数据源)
10. [Wikidata / 中文维基百科](#10-wikidata--中文维基百科)
11. [刮削器的多源合并与缓存实践](#11-刮削器的多源合并与缓存实践)
12. [媒体资源的版权与缓存注意事项](#12-媒体资源的版权与缓存注意事项)
13. [对自建刮削工具的可执行建议](#13-对自建刮削工具的可执行建议)
14. [来源清单](#14-来源清单)

---

## 执行摘要（TL;DR）

**十条最重要的结论**（每条都在正文有一手来源）：

1. **ScreenScraper 是唯一以「哈希 + 文件大小」为强制主键的源** —— 官方要求「除非获得豁免，你**必须**发送 crc/md5/sha1 之一**以及**文件字节大小」。这使它的匹配基于内容而非文件名，误匹配率结构性地低于其他源（§1.4）。
2. **但 ScreenScraper 有两道硬门槛**：devid 需论坛人工审批（实测无 devid 直接 HTTP 403），且 API「只能集成到完全免费且分发的应用中」。**商业化项目基本用不了**（§1.1、§1.2）。
3. **TheGamesDB 其实支持 ROM 哈希查询** —— `/v1/Games/ByGameHash` 支持 `md5`/`crc`，另有 `ByGameUniqueID` 支持 ROM serial。这与常见认知相反（§2.3）。但 TGDB **没有任何条款页面，处于法律真空**（§2.5）。
4. **GiantBomb API 已整体下线** —— 官方页面列出 Games/Characters/Companies/Releases/People 等全部 "Not currently available"。**从候选中划掉**（§7）。
5. **IGDB 商业使用免费且明确允许缓存**，但其引用的 Twitch 开发者协议禁止 re-syndication 且只允许 24 小时缓存 —— **两份文件字面冲突**，要分发离线数据包必须先取书面确认（§3.7）。
6. **⚠️ 绝不要直接抓 No-Intro Dat-o-Matic** —— 调研中单次参数畸形的请求就触发了**永久 IP 封禁**，需人工发邮件解除。其 robots.txt 声称允许抓取，**不可作为安全依据**。改用 GitHub 每日镜像（§8.1）。
7. **MAME `/hash` 是唯一法律完全干净的哈希源** —— `COPYING` 明写「The contents of the hash directory are **dedicated to the public domain**」，每个 XML 自带 `license:CC0-1.0` SPDX 头。代价：只有 CRC+SHA1，不含街机（§8.4）。
8. **Bangumi 是中文游戏元数据的最佳来源**，且**不是只做日系 galgame** —— 实测欧美主机游戏（Halo、GTA、Sonic、ToeJam & Earl）全部有条目、有中文译名、有中文简介。共 87,188 条游戏，有**每周三更新的 435 MB 离线 dump**。但老平台深度浅（N64 仅 98 条、DC 仅 38 条），且**dump 完全不含图片**（§9.1）。
9. **Wikidata 是跨库 ID 的枢纽，且是 CC0** —— 175,739 条游戏中 **80.8% 有 IGDB ID**、70.8% 有 Steam ID；中文标签覆盖 27–42%。但 **P18 图片覆盖率只有 3.4%，维基系不能作为封面图源**（Commons 明确拒绝合理使用内容）（§10）。
10. **没有任何中文来源支持 ROM 哈希查询。** ROM → 中文条目的映射必须自建：先用 DAT 做哈希识别得规范名，再用「名称 + 平台 + 年份」三重校验匹配到 Bangumi/Wikidata（§9、§13.1）。

**架构上最值得对标的实现是 Skyscraper** —— 它是唯一同时具备「多源 + **字段级**优先级 + 持久化缓存 + 离线再生成」四项能力的开源刮削器（§11.1、§11.5）。ES-DE **完全没有持久化结果缓存**，每次刮削都重打 API（§11.2）。

---

## 1. ScreenScraper.fr

一手来源：
- API v2 文档：<https://api.screenscraper.fr/webapi2.php>
- 注册页（含使用规则原文）：<https://www.screenscraper.fr/membreinscription.php>
- 站点页脚许可声明：<https://www.screenscraper.fr/faq.php>（页脚 `rel="license"` 指向 <http://creativecommons.org/licenses/by-nc-sa/4.0/>）
- Skyscraper 的 ScreenScraper 实现：<https://github.com/muldjord/skyscraper/blob/master/src/screenscraper.cpp>
- Skyscraper 模块说明：<https://github.com/muldjord/skyscraper/blob/master/docs/SCRAPINGMODULES.md>

### 1.1 注册要求 / 认证方式

**【事实】** API v2 的每个请求都需要**两层凭据**：

| 参数 | 含义 | 是否必填 |
|---|---|---|
| `devid` / `devpassword` | **开发者**凭据，标识调用方软件 | 必填 |
| `softname` | 调用方软件名称（会被服务端记录，可被拉黑） | 必填 |
| `ssid` / `sspassword` | **终端用户**的 ScreenScraper 站点账号 | 多数端点标注「非必填」，但实际强烈建议 |
| `output` | `xml`（默认）/ `json` / `ini` | 可选 |

来源：<https://api.screenscraper.fr/webapi2.php>（"Paramètres d'entrées" 小节，每个端点均列出）

**【事实】devid 的申请门槛是人工审批，不是自助注册。** 官方原文：

> « Si vous êtes développeur et voulez intégrer notre API, contactez-nous via le forum pour présenter votre logiciel et obtenir vos identifiants et mot de passe à fournir à l'API pour valider vos droits d'exploitation de celle-ci »
> （如果你是开发者并希望集成我们的 API，请通过论坛联系我们、介绍你的软件，以取得提供给 API 的开发者标识与密码来验证你的使用权。）

来源：<https://api.screenscraper.fr/webapi2.php>（"Qui peut utiliser l'API ?" 小节）

**【事实】终端用户账号是免费的**，但注册页明文列出封禁规则：

> "You are about to create a free account at ScreenScraper."
> "Do not create additional account(s) when you have exceeded your daily scrape quota."
> "Do not share your member account or you will be banned for life. We have set up a daily quota system that allows a certain number of requests per day for the same account **AND/OR the same IP address**."

来源：<https://www.screenscraper.fr/membreinscription.php>

> **要点**：配额同时按**账号**和**IP** 计。多账号轮换绕配额会导致账号与 IP 永久封禁。

**【事实】开源刮削器如何处理 devid**：Skyscraper 把 devid 硬编码为作者本人 `muldjord`，devpassword 用自写的 `StrTools::unMagic()` 做简单混淆后编译进二进制：

```cpp
QString gameUrl = "https://www.screenscraper.fr/api2/jeuInfos.php?devid=muldjord&devpassword="
  + StrTools::unMagic("204;198;236;130;203;181;203;126;191;167;200;198;192;228;169;156")
  + "&softname=skyscraper" VERSION + ...
```
来源：<https://github.com/muldjord/skyscraper/blob/master/src/screenscraper.cpp>（`getSearchResults`，第 73 行附近）

**【推断】** 自建工具若想合规使用 ScreenScraper，必须走论坛申请自己的 devid，并把 `softname` 设成自己工具的名字；直接复用别人的 devid 会连累对方被封（API 有 426「软件被拉黑」错误码专门对付这个）。

### 1.2 是否收费

**【事实】** API 本身不按次收费，但**只允许完全免费的应用集成**：

> « L'API ScreenScraper ne peut être intégré que dans les applications entièrement gratuites et distribuées, ou, dans le cas contraire, avec l'autorisation préalable et les conditions dictées par l'équipe de ScreenScraper. Tout manquement à cette règle pourra faire l'objet d'une coupure de compte, voir d'éventuelles poursuites judiciaires ! »
> （ScreenScraper API 只能集成到完全免费且公开分发的应用中；否则须事先取得 ScreenScraper 团队的授权并遵守其条件。违反此规则可能导致账号被切断，甚至面临法律追诉！）

来源：<https://api.screenscraper.fr/webapi2.php>

**【事实】** 付费（Patreon / Tipeee 捐助）与内容贡献都会**提高配额与线程数**，但这是「奖励」而非付费套餐：

> « Il existe 2 méthodes simples : - Participer à la base de données en proposant de nouvelles informations ou de nouveaux medias - Participer financièrement à l'hébergement de la base de données via Tipee ou Patreon. »

来源：同上（"Comment gagner des « Threads » ?"）

### 1.3 速率与配额限制

**【事实】** 三种限制并存：

1. **并发线程数（threads）** —— 同时进行的请求数，按用户贡献等级分配。
   - `ssuserInfos.php` 返回 `maxthreads`（该用户允许的线程数）与 `maxdownloadspeed`（允许的下载速率，KB/s）。
   - `ssinfraInfos.php` 返回全站的 `maxthreadfornonmember` / `threadfornonmember` / `maxthreadformember` / `threadformember`。
   - **实测**（2026-08-30 站点首页统计条）：`threads open: 263 / 4096`，`guest threads open: 48 / 256`。即**全站匿名用户共享 256 个线程**。
   - `contribution` 字段说明：`2 = 1 Thread Supplémentaire / 3 et + = 5 Threads Supplémentaires`（财务贡献等级 2 加 1 个线程，等级 3 及以上加 5 个线程）。

2. **每分钟 / 每天请求配额（quota）** —— 2019 年年中引入。`ssuserInfos.php` 与 `jeuInfos.php` 的响应里都回传：
   - `maxrequestspermin`：每分钟最大请求数
   - `maxrequestsperday`：每天最大请求数
   - `maxrequestskoperday`：**每天允许的「未命中」请求上限**（rom/jeu non trouvé）
   - `requeststoday` / `requestskotoday`：当日已用量

   官方明确要求客户端自行管理配额：
   > « Cette gestion de « Quota » par logiciel est désormais obligatoire afin de ne pas saturer nos serveurs pour rien. »
   > （由软件侧管理配额现在是**强制性**的。）

3. **服务端过载熔断** —— `closefornomember`（CPU>60% 时对匿名用户关闭 API）、`closeforleecher`（对零贡献会员关闭）。

来源：<https://api.screenscraper.fr/webapi2.php>

**【事实】HTTP 错误码语义**（官方表格）：

| 码 | 含义 |
|---|---|
| 400 | URL 缺参 / rom 文件名含路径（如 `!mnt!sda1!batocera!roms!...`）/ crc、md5、sha1 格式错误 |
| 401 | 服务器饱和（CPU>60%），对非会员或不活跃会员关闭 |
| 403 | 开发者凭据错误 |
| 404 | 未找到游戏 / ROM |
| 423 | API 完全关闭（服务器严重故障） |
| 426 | **调用的刮削软件已被拉黑**（不合规 / 版本过旧），必须换版本 |
| 429 | 达到该用户允许的线程数 / 每分钟线程数 / 匿名用户线程上限 |
| 430 | **当日 scrape 配额超限** |
| 431 | **当日「未识别 ROM」配额超限**（"Faite du tri dans vos fichiers roms et repassez demain !"） |

来源：<https://api.screenscraper.fr/webapi2.php>（"Retour d'erreurs"）

**【事实】** Skyscraper 文档给出的实践数字：`API request limit: 20k per day for registered users`，`Thread limit: 1 or more depending on user credentials`。
来源：<https://github.com/muldjord/skyscraper/blob/master/docs/SCRAPINGMODULES.md>

**【未确认】** ScreenScraper 官方 FAQ 页面（<https://www.screenscraper.fr/faq.php>）在未登录时**不渲染 FAQ 正文**，API 文档里三处「voir F.A.Q.」指向的具体配额分级表（各用户等级对应多少 threads / requests-per-day）无法从匿名一手来源取得。上面的 20k/天数字来自 Skyscraper 源码仓库文档，属于第三方实现者的记录。

**【事实】** Skyscraper 的错误处理直接把这些配额信号翻译成用户提示，可作为实现范例：

```cpp
} else if(headerData.contains("Votre quota de scrape est")) {  // 430
  printf("Your daily ScreenScraper request limit has been reached, exiting nicely...");
  reqRemaining = 0; return;
}
...
reqRemaining = maxrequestsperday - requeststoday;   // 每次响应都重算剩余额度
```
来源：<https://github.com/muldjord/skyscraper/blob/master/src/screenscraper.cpp>（第 75–150 行）

### 1.4 是否支持按 ROM 哈希查询 —— **支持，而且是首选路径**

**【事实】** 这是本次调研最关键的结论之一。`jeuInfos.php` 的输入参数：

| 参数 | 说明 |
|---|---|
| `crc` * | 本地 rom/iso/文件夹的 CRC32 |
| `md5` * | 本地 rom/iso/文件夹的 MD5 |
| `sha1` * | 本地 rom/iso/文件夹的 SHA1 |
| `romtaille` * | **文件或文件夹的字节大小** |
| `systemeid` | 平台数字 ID（见 `systemesListe.php`） |
| `romtype` | `rom` / `iso` / `dossier`(文件夹) |
| `romnom` | 文件名（含扩展名，URL 编码） |
| `serialnum` | 用光盘序列号强制匹配 |
| `gameid` | 用 ScreenScraper 数字 ID 强制匹配（此时不发送任何 rom 信息） |

带 `*` 的参数官方注解（**关键**）：

> « * a moins d'une dérogation, vous devez envoyer au moins un (le mieux serait les 3) de ces calculs (crc,md5,sha1) d'identification de fichier rom/iso ou dossier avec votre requete **ET la taille** (en octet du fichier ou du dossier). »
> （除非获得豁免，你**必须**在请求中至少发送这三种校验值之一（最好三个都发）**以及文件/文件夹的字节大小**。）

来源：<https://api.screenscraper.fr/webapi2.php>（jeuInfos.php 小节）

官方示例：
```
https://api.screenscraper.fr/api2/jeuInfos.php?devid=xxx&devpassword=yyy&softname=zzz
  &output=xml&ssid=test&sspassword=test
  &crc=50ABC90A&systemeid=1&romtype=rom
  &romnom=Sonic%20The%20Hedgehog%202%20(World).zip&romtaille=749652
```

**【事实】** 返回的 `rom` 对象也回传哈希，可用于本地校验与建索引：
`romid`、`romfilename`、`romserial`（厂商序列号）、`romregions`、`romlangues`、`romsize`、`romcrc`、`rommd5`、`romsha1`、`romcloneof`，以及 `beta` / `demo` / `trad` / `hack` / `unl` / `alt` / `best` / `retroachievement` / `gamelink` 布尔标记。同时 `roms[]` 数组给出该游戏**已知的全部 ROM 变体**。

**【事实】** Skyscraper 的实现印证了这一点（`getSearchNames`）：同时算 CRC32+MD5+SHA1 并连同 `romtaille` 一起发送；文件大小为 0 时退化为纯文件名查询：

```cpp
if(info.size() != 0) {
  searchNames.append("crc=" + ... + "&md5=" + ... + "&sha1=" + ...
                     + "&romnom=" + ... + "&romtaille=" + QString::number(info.size()));
} else {
  searchNames.append("romnom=" + hashList.at(0));
}
```
另有一个重要细节：当开启 `unpack` 时，Skyscraper 会用 `7z x -so` **把 zip/7z 解压到 stdout 后再算哈希**（因为 DAT 里的哈希是针对解压后内容的），且限制压缩包内必须只有 1 个文件、且体积在限制以内：

```cpp
// Size limit for "unpack" is set to 8 megs to ensure the pi doesn't run out of memory
if((info.suffix() == "7z" || info.suffix() == "zip") && info.size() < 81920000) { ... }
else { printf("File either not a compressed file or exceeds 8 meg size limit, falling back..."); }
```
（注：注释写 8 MB，实际常量是 `81920000` ≈ 78 MB，代码与注释不一致 —— 这是源码里的既有瑕疵。）
来源：<https://github.com/muldjord/skyscraper/blob/master/src/screenscraper.cpp>（第 487–548 行）

**【事实】** 另有 `jeuRecherche.php`：按游戏名模糊搜索，**按匹配概率排序，最多返回 30 条**，返回结构「与 jeuInfos 相同但不含 rom 信息」。这是哈希未命中时的 fallback。

### 1.5 媒体类型清单

**【事实】** `jeuInfos.php` 返回的 `medias` 节点包含（每一项都是可直接下载的 URL）：

| 媒体 | 说明 | 是否分区域 |
|---|---|---|
| `media_screenshot` | 游戏截图 | 否 |
| `media_fanart` | 同人/背景图 | 否 |
| `media_video` | 视频录像 | 否 |
| `media_marquee` | Marquee | 否 |
| `media_screenmarquee` | Screen Marquee | 否 |
| `media_wheel_xx` | 彩色 Logo（wheel） | **是** |
| `media_wheelcarbon_xx` | Wheel 碳纤维版 | 是 |
| `media_wheelsteel_xx` | Wheel 钢质版 | 是 |
| `media_boitier_texture_xx` | 包装盒展开贴图 | 是 |
| `media_boitier_2d_xx` | 2D 盒图 | 是 |
| `media_boitier_3d_xx` | 3D 盒图 | 是 |
| `media_support_texture_xx` | 卡带/光盘贴图 | 是（多碟加序号，如 `media_support_texture_fr1`） |
| `media_support_2d_xx` | 卡带/光盘 2D 图 | 是 |
| `media_flyer_xx` | 传单（多页加序号 `media_flyer_wor1`） | 是 |
| `media_manuel_xx` | **PDF 说明书** | 是 |
| `media_bezel4-3_xx` / `media_bezel16-9_xx` / `media_bezel16-10_xx` | 边框 bezel | 是 |

其中 `xx` 是 `regionsListe.php` 返回的 `nomcourt`（区域短名）。
另有厂商/分类相关的图标类媒体：`editeurmedia_*`、`developpeurmedia_*`、`genre_<id>_media_*`、`mode_<id>_media_*`、`famille_<id>_media_*`、`theme_<id>_media_*`、`classifications_<organisme>_media_*`、`notemedia_*`、`joueursmedia_*`。

来源：<https://api.screenscraper.fr/webapi2.php>（jeuInfos.php 的 `medias` 节点）

**【事实】媒体的增量下载协议**：`mediaJeu.php` 支持传入本地已有图片的 `crc`/`md5`/`sha1`，若与服务端一致则**只返回文本 `CRCOK`/`MD5OK`/`SHA1OK` 而不回传图片**（官方注释为 "optimisation des mises à jour"）；文件不存在时返回文本 `NOMEDIA`。还支持 `maxwidth`/`maxheight`/`outputformat`（png|jpg）做服务端缩放。

> 这是**为增量缓存量身定制的机制**，自建工具应该用起来 —— 可以把媒体更新的带宽成本降到接近零。

来源：同上（mediaJeu.php 小节）

**【事实】** 文本信息字段：`noms{nom_ss, nom_xx}`（按区域的标题）、`synopsis{synopsis_xx}`（**按语言的简介**）、`editeur`/`developpeur`、`joueurs`、`note`（20 分制）、`dates{date_fr,date_eu,date_us,date_jp,...}`、`genres`/`modes`/`familles`/`themes`/`styles`/`numeros`（均带多语言名）、`classifications`（分级）、`rotation`/`resolution`（街机）、`sp2kcfg`（Recalbox pad2keyboard 配置）、`tips`、`actions`、`couleurs`。

**【事实】ScreenScraper 支持中文（zh）简介与标签。** Skyscraper 的语言表把 `zh: Chinese` 列为 screenscraper 支持的语言之一（该表专门标注"支持情况在括号内"，且**只有 screenscraper 一个模块支持语言维度**）。
来源：<https://github.com/muldjord/skyscraper/blob/master/docs/LANGUAGES.md>

**【事实】** 区域维度上 Skyscraper 列出 screenscraper 支持 `cn: China` 与 `tw: Taiwan`。
来源：<https://github.com/muldjord/skyscraper/blob/master/docs/REGIONS.md>

**【推断】** ScreenScraper 的中文覆盖率**很可能远低于**法/英/西/德，因为它是法国社区驱动、内容靠会员投稿；`zh` 字段存在 ≠ 大部分条目有中文。**未能从一手来源确认实际中文覆盖率**（需要 devid 才能查询验证）。

### 1.6 授权条款：第三方工具使用与缓存

**【事实】** 站点全站页脚声明：

```html
<a rel="license" href="http://creativecommons.org/licenses/by-nc-sa/4.0/">
licensed under the terms of Creative Commons</a><br>
Attribution-NonCommercial-ShareAlike 4.0 International
```
且 `<meta name="description">` 写着 "…faciliter la collecte et la **redistribution communautaire libre (Creative Commons)** des données et des médias des jeux vidéo rétro."

来源：<https://www.screenscraper.fr/faq.php>（页脚 HTML，全站模板通用）

**结论**：
- **数据与媒体按 CC BY-NC-SA 4.0 授权**：允许缓存与再分发，但必须**署名**、**禁止商业用途**、**衍生作品同样以 BY-NC-SA 授权**。
- **API 使用另有独立限制**：只能集成进「完全免费且分发的应用」，商业集成须事先授权（见 §1.2）。
- 页脚同时列出上游素材来源（Wikipedia、GameFAQs、jeuxvideo、MobyGames、The Cover Project、GBAtemp、Hyperspin、EmuMovies、Progetto Snaps、flyers.arcade-museum.com 等），**【推断】** 这意味着 ScreenScraper 上的部分媒体本身是转载的第三方素材，CC BY-NC-SA 的声明对这些素材的效力存疑。

> **【推断】对自建工具的意义**：若工具本身免费开源、且只在**用户本机**缓存数据，风险很低；若打算做**云端集中缓存并对外提供**，就同时踩到了 NC（非商业）与 API 条款两条线，必须先与 ScreenScraper 团队联系。

---

## 2. TheGamesDB (TGDB)

一手来源：
- Swagger UI：<https://api.thegamesdb.net/>
- OpenAPI spec（swagger 2.0, version 2.0.0）：<https://api.thegamesdb.net/spec.yaml>
- API Key 页：<https://api.thegamesdb.net/key.php>
- 注册页：<https://thegamesdb.net/register.php>
- 提额页：<https://thegamesdb.net/increase_usage.php>

### 2.1 注册要求 / 认证方式 / 是否收费

**【事实】** 必须注册站点账号（免费）并登录后才能取 key。`key.php` 页面全文只有一句：
> "You must be logged in to the site to view your api key."

**【事实】** 认证方式是 **query 参数 `apikey`**（不是 header），spec 中每个端点都标 `(Required)`。不带 key 请求 `https://api.thegamesdb.net/v1/Games/ByGameName` 实测返回 `{"code":418,"status":"Unknown Error Code"}`。

**【事实】** 免费。官网无付费档位页面，只有 Patreon 捐赠入口 <https://www.patreon.com/thegamesdbnet>。提额走 `increase_usage.php`（需登录）。

### 2.2 速率与配额

**【事实】** 每个响应都带三个配额字段：`remaining_monthly_allowance`、`extra_allowance`、`allowance_refresh_timer`（spec 示例值 249 / 0 / 2592000 秒 = 30 天）。

`allowance_refresh_timer` 的官方描述是区分 public key 与 private key 的**唯一一手证据**：
> "Seconds until monthly allowance resets. **null for private/unlimited keys.**"

**【事实】** `/v1/API/Limit` 可查余额，官方注明 "Does not count against your allowance."。超限返回 403（"bad API key or hit rate-limit cap"）。

**【未确认】** "public key 1500 次/月、private key 6000 次" 之类的具体数字**无法从一手来源取得**：官方论坛 <https://forums.thegamesdb.net/> **整站被 nginx HTTP Basic Auth 锁定，任何路径均返回 401**。
**【事实】** 第三方实现者记录的数字：Skyscraper 文档写 `Limited to 3000 requests per IP per month`（<https://github.com/muldjord/skyscraper/blob/master/docs/SCRAPINGMODULES.md>）。

### 2.3 是否支持 ROM 哈希查询 —— **支持**（重要发现，与常见认知相反）

**【事实】** spec 中存在：

- `GET /v1/Games/ByGameHash` — summary："Fetch game(s) by ROM hash"
  - 参数：`hash`（必填）、`filter[platform]`、`filter[type]`（"(Optional) - hash type (e.g. `md5`, `crc`)"）
  - 官方示例：`.../ByGameHash?apikey=APIKEY&hash=a29da82b&filter[type]=crc&page=1`
- `GET /v1/Games/ByGameUniqueID` — "Search for games by their unique/external identifier (**e.g. ROM serial**)"，示例 `uid=GM-007037-00`

来源：<https://api.thegamesdb.net/spec.yaml>

**【推断】** 接口存在 ≠ 数据覆盖率高。TGDB 定位是「按名字搜索的元数据库」，其哈希索引的实际填充度大概率远不如 ScreenScraper（后者的哈希是核心识别机制且强制上传）。**【未确认】** 实际覆盖率无 key 无法验证。

### 2.4 媒体类型

**【事实】**
- 游戏图片 `/v1/Games/Images`，`filter[type]` ∈ `fanart`, `banner`, `boxart`, `screenshot`, `clearlogo`, `titlescreen`；boxart 带 `side` 字段（`front`/`back`）；每条含 `filename` 与 `resolution`。
- `base_url` 六种尺寸：`original` / `small` / `thumb` / `cropped_center_thumb` / `medium` / `large`，均为 `https://cdn.thegamesdb.net/images/{size}/`，拼上 `filename` 即完整 URL。
- 平台图片 `/v1/Platforms/Images` 只有 `fanart`, `banner`, `boxart`。
- 视频 `/v1/Games/Videos`，`base_url` = `https://cdn.thegamesdb.net/`，filename 形如 `videos/53/53-1.mp4`。

**【事实】文本字段**（`fields` 参数可选）：`players`, `publishers`, `genres`, `overview`, `last_updated`, `rating`, `platform`, `coop`, `youtube`, `os`, `processor`, `ram`, `hdd`, `video`, `sound`, **`alternates`**（别名，可能含中文名；**【未确认】** 覆盖率）。`include` 可选 `boxart`, `platform`。

**【事实】增量同步**：`/v1/Games/Updates` 支持 `last_edit_id`，可做增量拉取 —— 对本地缓存很关键。

**【事实】版本状态**：旧版 XML API（`https://thegamesdb.net/api/*.php`）已退役（`GetGame.php`、`GetGamesList.php` 返回残桩 `<?xml update your programs`，`GetArt.php`/`GetPlatformsList.php` 返回 403）。当前 `api.thegamesdb.net` 的 v1 仍在服役并新增 v1.1；spec 注记：
> "in v1 any value passed to the `mode` param triggers natural language search. **Use `/v1.1/Games/ByGameName` for correct `mode=natural` handling.**"

### 2.5 授权条款 —— **法律真空**

**【事实】** TheGamesDB **官网不存在任何 ToS / 条款 / 许可页面**。逐一探测 `/terms.php`、`/terms`、`/tos.php`、`/legal.php`、`/about.php`、`/privacy.php`、`/faq.php` **全部 404**；首页无条款入口；唯一法律性文字是页脚 "© 2026 TheGamesDB"。

**【事实】** spec 里的 `license` 字段写着 **GNU GPL v3.0**，指向 <https://github.com/TheGamesDB/TheGamesDBv2/blob/master/LICENSE> —— 这是**网站软件代码**的许可（该 repo 确为 GPL-3.0），**不是数据/图片的许可**。

> **结论**：TGDB 对第三方工具使用、缓存、图片再分发**没有成文条款**。**【推断】** 自用缓存风险低；但把 TGDB 图片打包再分发处于「无明示授权」状态，产品化有风险。论坛可能有站方表态，但已 401 锁死无法取证。

---

## 3. IGDB / Twitch API

一手来源：
- <https://api-docs.igdb.com/>
- <https://dev.twitch.tv/docs/authentication/getting-tokens-oauth/>
- <https://legal.twitch.com/en/legal/developer-agreement/>

### 3.1 注册要求 / OAuth 流程

**【事实】** 需要 Twitch 账号，且 **2FA 是硬性要求**。官方 "Account Creation" 步骤原文：
> "In order to use our API, you must have a Twitch Account." / "Sign Up with Twitch for a free account" / "**Ensure you have Two Factor Authentication enabled**" / "Register your application in the Twitch Developer Portal" / "The OAuth Redirect URL field is not used by IGDB. Please add 'localhost' to continue." / "The Client Type must be set to **Confidential** to generate Client Secrets"

**【事实】** OAuth2 client_credentials 流程：
```
POST https://id.twitch.tv/oauth2/token?client_id=X&client_secret=Y&grant_type=client_credentials
→ {"access_token": "...", "expires_in": 5587808, "token_type": "bearer"}
```
随后每个 API 请求带两个头（官方强调大小写与硬编码前缀）：
```
Client-ID: {Client ID}
Authorization: Bearer {access_token}
```
Base URL `https://api.igdb.com/v4`，**绝大多数请求用 POST**，Apicalypse 查询语句放 body。

**【事实】** Token 生命周期：
> "Your Access Token is only active for **60 days** and your application can only have **25 active Access Tokens** at one time, going over this limit starts to inactivate older tokens."

**【事实】** 不支持浏览器直连：
> "The IGDB API does not support browser requests, CORS, for security reasons. This is because the request would leak your access token!"

→ **必须自建后端代理**。

### 3.2 是否收费 / 速率

**【事实】** 免费，且**商业用途也免费**（与常见认知相反）：
> "The API is free for both non-commercial and commercial projects."

**【事实】** 速率（原文）：
> "There is a rate limit of **4 requests per second**. If you go over this limit you will receive a response with status code 429 Too Many Requests."
> "You are able to have up to **8 open requests** at any moment in time."

默认返回 10 条，`limit` 最大 **500**。

### 3.3 是否支持 ROM 哈希查询 —— **不支持**

**【事实】** 对整份文档做全文检索，`md5`/`crc`/`sha1`/`rom hash` **全部无匹配**。文档里的 `checksum` 字段含义是 "Hash of the object"（记录对象哈希，用于变更检测），与 ROM 无关。

### 3.4 Apicalypse 查询语言 & 关键字段

**【事实】**
```
fields name, cover.*, alternative_names.*;   // 选字段，* 全部；展开关联对象
exclude ...;
where rating > 75 & platforms = 48;          // & 与，| 或
sort rating desc;  limit 50;  offset 10;  search "mario";
```
支持 Multi-Query（单请求打多端点）与 Protocol Buffers 响应。

**关键端点/字段**：
- `games`：`alternative_names`, `artworks`, `covers`, `screenshots`, `videos`, `game_localizations`, `release_dates`, `franchises`, `involved_companies`, `external_games`, `language_supports`, `keywords`, `themes`, `collections`, `game_type`, `game_status`
- `alternative_names`：`name`, `game`, **`comment`**（"A description of what kind of alternative name it is (Acronym, Working title, **Japanese title** etc)"）
- `game_localizations`：`name`, `game`, **`region`**, `cover`（"A region can have at most one game localization for a given game"）
- `covers`/`artworks`/`screenshots`：`image_id`, `width`, `height`, `url`, `image_type`；`covers` 另有 `game_localization` 字段 → **可取本地化封面**
- `game_videos`：`video_id`（YouTube ID）
- `involved_companies`：`developer`/`publisher`/`porting`/`supporting` 布尔位

**【事实】`external_games` 只映射到商店/平台，不映射到其他元数据库。** 可用 source：steam(1), gog(5), youtube(10), microsoft(11), apple(13), twitch(14), android(15), amazon_asin(20), amazon_luna(22), amazon_adg(23), epic_game_store(26), oculus(28), utomik(29), itch_io(30), xbox_marketplace(31), kartridge(32), playstation_store_us(36), focus_entertainment(37), xbox_game_pass_ultimate_cloud(54), gamejolt(55)。
> **没有 TGDB / MobyGames / GiantBomb 的 ID 映射** —— 跨库对齐仍需自己按 标题+平台+年份 做。

### 3.5 中文标题覆盖 —— 有实证，但结构不理想

**【事实】** 在 IGDB 官网条目 <https://www.igdb.com/games/black-myth-wukong>（与 API 同源数据）实测可见：
- **Alternative Titles**：`Chinese title - simplified: 黑神话：悟空`、`Japanese title - stylized: 黒神話：悟空`、`Alternative spelling: Black Myth: Wu Kong`
- **Localized Titles**：`Korea: 검은 신화: 오공`、`Japan: 黒神話：悟空`（**没有 China 条目**）
- **Supported Languages**：`Chinese (Simplified)` / `Chinese (Traditional)`，分 Audio / Subtitles / Interface

**结论**：
- 简体中文标题主要落在 `alternative_names`，**靠 `comment` 字符串区分**（值形如 `"Chinese title - simplified"`），**不是**结构化的 zh-CN/zh-TW 语言码 → 需要按字符串匹配 comment，脆弱且不统一。
- `game_localizations` 走 `region` 引用，此例无 China → **zh 走 localization 的覆盖率偏低**。
- `languages` 端点承载的是「游戏支持哪些语言」（"Languages that are used in the Language Support endpoint."），**不承载标题翻译**。
- **【未确认】** 中文标题在全库的覆盖率百分比（需 key 做统计查询）。

### 3.6 图片 URL 与缓存过期

**【事实】**
```
https://images.igdb.com/igdb/image/upload/t_{size}/{image_id}.jpg
```
尺寸名：`cover_small` 90×128、`screenshot_med` 569×320、`cover_big` 264×374、`logo_med` 284×160、`screenshot_big` 889×500、`screenshot_huge` 1280×720、`thumb` 90×90、`micro` 35×35、`720p` 1280×720、`1080p` 1920×1080；追加 `_2x` 得 retina。API 默认只回 `t_thumb`，需自行替换尺寸。

**【事实】缓存提醒（原文）**：
> "Images that are removed or replaced from IGDB.com exist for **30 days** before they are removed. Keep that in mind when designing cache logic."

### 3.7 授权条款 —— **存在文本冲突，必须注意**

**【事实】IGDB 文档侧非常宽松**（<https://api-docs.igdb.com/>）：
> "**Am I allowed to store/cache the data locally?** Yes. In fact, we prefer if you store and serve the data to your end users."
> "I want to use the API for a commercial project, is it allowed? Yes, we offer commercial partnerships... we ask for **user facing attribution to IGDB.com**"（联系 `partner@igdb.com`）
> "We expect fair attribution, i.e. attribution that is **visible to your users and located in a static location** (e.g. not in a change log)."
> "**You are allowed to keep all data you retrieve** from the API and we will not ask you to remove the data in case of partnership termination."
> "The IGDB.com API is free for non-commercial usage **under the terms of the Twitch Developer Service Agreement**."

**【事实】但被引用的 Twitch 开发者协议要严得多**（<https://legal.twitch.com/en/legal/developer-agreement/>）：
> "**Do not store copies of Twitch Content or Program Materials**, unless you: (a) obtain prior written authorization from Twitch (through these terms or otherwise); (b) control the rights associated with such content; or (c) **cache such information for only a twenty-four hour time period** without further sharing it with third parties. **Re-syndication and re-distribution of Program Materials or data as available from a Twitch API is prohibited.**"

**【推断】解读**：IGDB FAQ 里「可以长期存储」的表态可视为 Twitch 协议 (a) 项所说的书面授权。但两份文件字面矛盾，且**「再分发/再联合发布被禁止」这条没有被 IGDB FAQ 推翻**。即：**本地缓存自用 + 对自己终端用户展示 OK；把数据打包再分发给第三方（例如发布离线元数据包）不 OK**，需先向 `partner@igdb.com` 取书面确认。

**【事实】Data Partner 福利**：签约后可拿**每 24 小时更新的全量 CSV data dumps**（`GET /v4/dumps`、`GET /v4/dumps/{endpoint}`，S3 预签名链接有效 5 分钟，带 `schema_version`）。原文："Please note that data dumps are exclusively available to our Data Partners."

**【事实】迁移警告**：文档有 "Migration Enums to Tables" 章节，enum 正改为表结构并改名：`external_game.category`→`external_game_source`、`external_game.media`→`game_release_format`、`platform.category`→`platform_type`、`website.category`→`type`、`game.category`→`game_type`、`game.status`→`game_status`。迁移期 "starting on February 18 to August 31 (6 months)"（**【未确认】** 文档未标年份）。写新代码请直接用新字段名。

---

## 4. LaunchBox Games Database

一手来源：<https://gamesdb.launchbox-app.com/>、<https://gamesdb.launchbox-app.com/Metadata.zip>、官方论坛。

### 4.1 有无公开 API —— **没有，只有可下载数据包**

**【事实】** 官方论坛版主 JoeViking245 对 "LaunchBox DB API" 的答复是 "I'm not sure if Jason has an API setup for the site"，并指向本地 `Metadata.xml` 作为替代。
来源：<https://forums.launchbox-app.com/topic/52709-launchbox-db-api/>

### 4.2 下载地址与内容（实测下载解包确认）

**【事实】** `https://gamesdb.launchbox-app.com/Metadata.zip` —— HTTP 200，**无需登录、无需注册、免费**。
```
content-length: 107347481          (102 MiB)
last-modified: Sun, 30 Aug 2026 08:00:43 GMT   （调研当天，印证每日更新）
content-type: application/x-zip-compressed
```

解压后**恰好 4 个文件**（共 544 MiB）：

| 文件 | 解压后大小 | 记录数 |
|---|---|---|
| `Metadata.xml` | 486 MiB | 见下 |
| `Mame.xml` | 44 MiB | 48,781 × `MameFile` |
| `Files.xml` | 13.6 MiB | 85,636 × `File` |
| `Platforms.xml` | 301 KiB | 190 × `Platform` |

`Metadata.xml` 内记录类型与数量：`GameImage` ×1,324,332、`Game` ×187,732、`GameAlternateName` ×70,004、`PlatformAlternateName` ×431、`Platform` ×190、`EmulatorPlatform` ×98、`Emulator` ×35。

**【事实】** 官方论坛工作人员 SentaiBrad 确认该 URL 由作者 Jason Carr 提供，**约每 24 小时更新一次**。
来源：<https://forums.launchbox-app.com/topic/35162-any-way-to-download-metadata-manually/>

### 4.3 `Game` 元素的实际字段与填充率（实测）

**【事实】**
```
Name(100%) Cooperative(100%) DatabaseID(100%) Platform(100%)
CommunityRatingCount(100%) Genres(100%) ReleaseType(94.8%)
Overview(93.3%) Developer(90.9%) Publisher(88.5%) MaxPlayers(74.7%)
ESRB(72.6%) CommunityRating(64.1%) ReleaseDate(60.2%) VideoURL(43.2%)
ReleaseYear(35.8%) WikipediaURL(25.1%) SteamAppId(12.2%) DOS(2.9%)
StartupFile/StartupMD5(0.4%) SetupFile/SetupMD5(0.1%)
```

**三处需要注意的更正（与常见预期不符）**：

1. **【事实】`Game` 没有 `PlayMode` 字段。** `PlayMode` 只存在于 `Mame.xml` 的 `<MameFile>`（87.6% 填充，值如 `2P alt`）。`Game` 用 `Cooperative`(bool) + `MaxPlayers`(int) 表达同类语义。
2. **【事实】`Genres` 是单个元素内以 `; ` 分隔的字符串**，不是重复元素：`<Genres>Action; Adventure; Horror</Genres>`。
3. **【事实】`DatabaseID` 与网站 URL 里的 ID 不同**，仅用于包内互相关联。

### 4.4 是否含 ROM 哈希 —— **不含**（重要）

**【事实】** `Files.xml` 的 `<File>` 只有三个字段，100% 填充：
```xml
<File>
    <Platform>3DO Interactive Multiplayer</Platform>
    <FileName>20th Century Video Almanac (USA)</FileName>
    <GameName>20th Century Video Almanac (USA)</GameName>
</File>
```
这是**文件名 → 游戏名**的映射表（基于 No-Intro/Redump 命名），**不是哈希表**。全库唯一的 CRC 是 `<GameImage><CRC32>`，那是**图片文件自身的校验和**，与 ROM 无关。

> **结论**：LaunchBox **不能用于哈希识别 ROM**，必须先按文件名匹配再取元数据。

### 4.5 图片 CDN（实测确认拼接方式）

**【事实】** `GameImage.FileName` 直接拼到 `https://images.launchbox-app.com/` 之后：
```
https://images.launchbox-app.com/41ba4239-f39b-4428-8568-4b09d46c23df.jpg
→ HTTP/2 200, content-type: image/jpeg   ✅ 实测
```
`<GameImage>` 字段：`DatabaseID`、`FileName`、`Type`、`CRC32`（均 100%）、`Region`（63.1%）。
`Type` 主要取值：`Screenshot - Gameplay`(247k)、`Box - Front`(205k)、`Clear Logo`(143k)、`Screenshot - Game Title`(108k)、`Box - Back`(105k)、`Box - 3D`(93k)、`Fanart - Background`(71k)、`Disc`(51k)、`Cart - Front`(40k)、`Arcade - Marquee`(8.4k) 等。

### 4.6 许可 —— **无正式条款，只有作者的论坛默许**

**【事实】** `launchbox-app.com` 全站唯一的法律文件是隐私政策 PDF（`/Resources/Documents/launchbox-privacy-policy.pdf`），**没有 ToS、EULA 或数据许可页**。

**【事实】** 现存唯一的官方立场是 LaunchBox 作者 Jason Carr 在论坛的原文：
> "Apparently there's some faulty info out there. **There are at least a couple other apps using the LaunchBox Games Database already.**"
> "This just simply isn't true lol. I don't know how you can look at the metadata package and conclude that, since it includes metadata for all of the images. **It includes the image file names, which can be easily used to construct a URL.** The IDs don't have to match up to the website URLs; they only have to match up to the rest of the metadata in the zip."
> — Jason Carr, 2020-03-27, <https://forums.launchbox-app.com/topic/54163-is-there-a-public-way-to-get-images-from-the-launchbox-games-database/>

**【事实】** robots.txt 无任何 `Disallow` 指令。

> **【推断】风险评估**：Jason Carr 的表态是「默许/欢迎」的语气，但**在法律意义上不构成许可授予**。数据由社区贡献、图片源自各发行商的封面扫描。建议：可用于查询与本地展示，**不要整包再分发图片**。

---

## 5. libretro-database / RetroArch / libretro-thumbnails

### 5.1 libretro-database

一手来源：<https://github.com/libretro/libretro-database>

| 项目 | 结论 |
|---|---|
| 获取方式 | git clone / raw；RetroArch 内置 Online Updater |
| 注册 / 收费 | 均不需要 |
| 自动化限制 | 仅 GitHub 常规限制（未认证 API 60 req/h） |
| **哈希** | **CRC / MD5 / SHA1 / serial 全部具备** ✅ |
| 媒体 | **无** —— 图片在独立的 libretro-thumbnails |
| 许可 | **CC BY-SA 4.0**（LICENSE 文件实测确认） |

**【事实】仓库结构**：`cht/`（金手指）、`cursors/`（播放列表查询）、`dat/`（libretro 自维护 DAT）、`metadat/`（第三方 DAT：no-intro、redump、tosec、mame、developer、genre、publisher、bbfc、elspa、hacks、homebrew…）、`rdb/`（编译产物）、`scripts/`、`LICENSE`、`Makefile`、`README.md`。

**【事实】优先级规则**（README 原文）：
> "Databases earlier in the list have precedence over items later in the list. E.g. definitions in `/dat` will over-ride `/metadat` in the final `.rdb` compile if any info conflicts for the same game (i.e. for the same key field)."

**【事实】哈希字段确认**（README 原文）：
> "**For reasons of informational completeness, future-proofing, and compatibility outside RetroArch, databases contain checksum and cryptographic hashes regardless of the key used for matching.**"

实测 `metadat/no-intro/Nintendo - Nintendo DS.dat`：
```
game (
	name "007 - Blood Stone (France)"
	region "France"
	serial "BJBF"
	rom ( name "007 - Blood Stone (France).nds" size 67108864 crc 1FCCF595
	      md5 9B2132E18451CC31DFF255681DBBBA00
	      sha1 0E49395EDC31C21C0814C03A4893A1E86DE9F856 serial "BJBF" )
)
```
实测 `metadat/redump/Sony - PlayStation.dat`：
```
game (
	name "'98 Koushien - Koukou Yakyuu Simulation (Japan)"
	region "Japan"
	serial "SLPS-01204"
	rom ( name "...bin" size 583415952 crc 8ACD8FB1 md5 39A936EA7521157838D4E67B24F62F15
	      sha1 782C50827BF4CF8FE5530B64B188A2D43C75B0E0 serial "SLPS-01204" )
)
```

> **重要**：libretro 的 redump 转换版**额外补入了 serial**，而 **Redump 官方 DAT 本身没有 serial**（见 §8.2）。这使 libretro 镜像在光盘系统上反而比上游更好用。

**【事实】DAT → RDB 编译**（<https://github.com/libretro/RetroArch/blob/master/libretro-db/README.md>）：
```bash
c_converter "NAME_OF_RDB_FILE.rdb" "NAME_OF_SOURCE_DAT.dat"
c_converter "OUT.rdb" "rom.crc" "dat1.dat" "dat2.dat" "dat3.dat"   # 多 DAT 合并，以 CRC 为唯一指纹
./libretro-build-database.sh                                        # 全量构建（来自 libretro-super）
```
> "Files specified later in the chain **will override** earlier ones if the same key exists multiple times."

**【事实】RDB 二进制格式 —— 无独立规格文档，源码即规格。** 权威定义在 <https://github.com/libretro/RetroArch/blob/master/libretro-db/libretrodb.c>：
```c
#define MAGIC_NUMBER "RARCHDB"          // 文件头魔数
typedef struct libretrodb_header {
   char magic_number[sizeof(MAGIC_NUMBER)];   // 8 字节
   uint64_t metadata_offset;                  // 大端序
} libretrodb_header_t;
struct libretrodb_index { char name[50]; uint64_t key_size; uint64_t next; uint64_t count; };
#define LIBRETRODB_MAX_KEY_SIZE 256     // 注释："SHA-1 is 20 bytes"
```
记录本体是 **MessagePack 编码**（`rmsgpack.c` 是 RetroArch 自实现的 MessagePack）。写入流程：头部占位 → 逐条写 msgpack map → 末尾写元数据 map（`{"count": N}`）→ 回填 header。
注意：`libretro-db/*.c/h` 每个文件头带**独立的 MIT 许可声明**，与数据的 CC BY-SA 4.0 是两回事。

**【事实】读取 RDB 的官方 CLI `libretrodb_tool`**：
```bash
libretrodb_tool <db file> list
libretrodb_tool <db file> create-index <index name> <field name>
libretrodb_tool <db file> find "{'crc':b'31B965DB'}"
libretrodb_tool <db file> find "{'name':glob('Street Fighter*')}"
```
官方 README："Hash matching query. Usecase: Search for any game matching a given crc32... **Also works with serial, md5, and sha1.**"

**【事实】** GitHub 上**未找到成熟的纯 Python RDB 解析器**。唯一相关的 <https://github.com/Swordfish90/libretro-sqlite-db>（Python，3 stars，2022-06）README 明确说明是**包装官方 CLI 而非重新实现格式**。其余同名搜索结果都是 **Redis** 的 .rdb 解析器（libretro README 特意注明 "_no relation to Redis .RDB files_"）。

> **【推断】实务建议**：与其解析 RDB，不如**直接解析 `/metadat` 下的 clrmamepro `.dat` 文本** —— 格式简单、含全部哈希、无需 C 工具链。RDB 只是为 RetroArch 嵌入式平台优化的产物。

**【事实】RetroArch 扫描机制（CRC32 vs Serial）**，README 原文：
> "The key field for matching varies by console typical file size (i.e. original media type).
> — **CRC checksum** for systems with smaller file sizes, e.g. games before the advent of disc-based consoles.
> — **Serial Number** for larger files like disc-based games, to avoid computing checksums on large files. Found within the ROM file... scanned (in applicable cases) as a byte array by RetroArch.
> **CRC and serial also serve as RetroArch's primary index.**"

docs.libretro.com 进一步说明：严格自动扫描要求 "the content CRC checksum or disc serial must match existing databases from the libretro-database"；另有 "loose scan" 放宽此要求，但会失去缩略图与 Explore 菜单支持。每系统实际用哪种键见 <https://github.com/libretro/libretro-super/blob/master/libretro-build-database.sh>。

**【事实】许可**：`LICENSE` 首行 `Attribution-ShareAlike 4.0 International` → **CC BY-SA 4.0**。义务：署名 + 衍生作品同样以 CC BY-SA 4.0（或兼容许可）发布。**ShareAlike 的传染性对闭源商业产品是实质约束。**

### 5.2 libretro-thumbnails（美术资源）

一手来源：<https://github.com/libretro-thumbnails/libretro-thumbnails>、<https://thumbnails.libretro.com/>

**【事实】** libretro-database 本身**不含任何美术资源**（根目录只有 `cht/ cursors/ dat/ metadat/ rdb/ scripts/`，实测确认）。

**【事实】目录结构是 4 类而非常说的 3 类**（README 原文）：
> - `Named_Boxarts` are scans of the boxes or covers of games
> - `Named_Logos` are the logos for the games (not present for all systems)
> - `Named_Snaps` are in-game snapshots, aka gameplay screenshots
> - `Named_Titles` are images of the game's introductory title screen

路径约定 `thumbnails/Playlist Name/Named_Type/Game Name.png`。仓库为**每系统一个 git submodule**：
```bash
git clone --recursive --depth=1 http://github.com/libretro-thumbnails/libretro-thumbnails.git thumbnails
```

**【事实】命名映射到 No-Intro 名称**：图片名 = RetroArch 播放列表中的游戏名（即 libretro-database 赋予的规范名，源自 No-Intro/Redump），并做非法字符替换：
> "_Invalid characters._ If the characters ``&*/:`<>?\|"`` appear in a game name displayed in a playlist, they must be replaced with `_` in the corresponding thumbnail filename."

RetroArch 1.17.0+ 有三级柔性匹配：① ROM 文件名 → ② 播放列表游戏名 → ③ 截断括号后的短名（忽略区域/年份）。

README 特别强调**缩略图不由校验和关联**：
> "Thumbnails are __not__ directly assigned by the database or by checksum association, but as a secondary effect of databased *game name* assignment."

**【事实】缩略图服务器**（实测）：
```
https://thumbnails.libretro.com/Nintendo%20-%20Super%20Nintendo%20Entertainment%20System/Named_Boxarts/Super%20Mario%20World%20(USA).png
→ HTTP/1.1 200 OK, content-type: image/png, content-length: 301072   ✅
```
同步频率（README）：
> "The libretro-thumbnail server receives updates from the repository about once every two days on a cronjob. If you don't see updated files, append `?nocaches=CURRENTDATE` to have CloudFlare serve new content."

格式限制：**必须 PNG**；原生宽度 >512px 需缩放到 512px 宽。

**【事实】许可 —— 无 LICENSE 文件（风险最高）**：`https://raw.githubusercontent.com/libretro-thumbnails/libretro-thumbnails/master/LICENSE` 返回 **404**，GitHub API 目录列表中根目录只有 `.gitignore`、`.gitmodules` 和各系统 submodule。README 唯一相关声明：
> "*The game art itself and promotional art originates from the work of each respective game's developers and publishers.*"

且致谢中列出图片来源包括 **MobyGames** 与 **Fandom**（各有自己的版权条款）。

> **【推断】风险评估**：这是全部调研对象中**版权状态最不明确**的。libretro-database 的 CC BY-SA 4.0 **不覆盖**这个独立仓库。建议按需实时拉取、**不做整包再分发**。

---

## 6. MobyGames API

一手来源：<https://www.mobygames.com/info/api/>、<https://www.mobygames.com/api/subscribe/>、<https://www.mobygames.com/info/terms/>

### 6.1 注册 / 认证 / 收费

**【事实】** 需注册账号并**订阅付费计划**：
> "You can subscribe to our API to get an API key. We have options for hobbyists as well as users with a commercial application in mind."

Key 在 <https://www.mobygames.com/mobypro/api/> 查看。认证是 query 参数：`https://api.mobygames.com/v1/games?api_key=YOUR_KEY`。
官方特别提醒：
> "All query arguments (**including the api key!**) must be urlencoded. If you don't urlencode an api key with a `+` in it, the key will not be valid."

**【事实】收费档位**（<https://www.mobygames.com/api/subscribe/>，月付价，年付送 2 个月）：

| 档位 | 速率 | 月费 | 商业授权 | 主要内容 |
|---|---|---|---|---|
| Hobbyist | 1 req/s | $9.99 | **仅限非商业** | 平台/游戏/描述/发行日期/开发发行商/类型/封面/截图/Moby Score/官网 |
| Bundle: MobyPlus + Hobbyist | 0.2 req/s | $12.99 | **仅限非商业** | 上述 + MobyPlus 权益（全分辨率图、CSV/JSON 导出等） |
| Bronze | 1 req/s | $99.99 | **允许商业** | 加 Companies、Promo Images、Video Links、Critic/Player Score |
| Silver | 4 req/s | $499.99 | 允许商业 | 加 Groups、**Identifiers**、**Product Codes**、**AKAs**、**原始全尺寸图片**、评论、技术规格 |
| Gold | 8 req/s | $4,999.99 | 允许商业 | 加 110 万人物 Credits、Trivia、Player Reviews |

**【事实】免费通道**：非商业**研究**用途可申请免费：
> "Yes, to request free API access for non-commercial research purposes, please complete this form with details about your project and the data you require."

### 6.2 速率与配额

**【事实】** 文档原文（与订阅页略有出入）：
> "Non-commercial API requests are limted to **720 per hour** (one every five seconds) with a max request rate of **1 per/second**."
> "Legacy non-commercial API requests are limited to **360 per hour** (one every ten seconds)."

超限返回 429，消息体："Oy, you're making requests too frequently for us to handle. Please wait at least 1 seconds between requests."

**【事实】** Skyscraper 文档记录的实践：`API request limit: 1 request per 10 seconds`，`Rom limit per run: 35`，并警告该限制是「全体 Skyscraper 用户共享」：
> "This restriction is **global for the entire Skyscraper user base**… Please use this module sparingly."

来源：<https://github.com/muldjord/skyscraper/blob/master/docs/SCRAPINGMODULES.md>

### 6.3 是否支持 ROM 哈希查询 —— **不支持**

**【事实】** 完整端点清单无任何 hash/checksum 接口：
`/genres`、`/groups`、`/platforms`、`/games`、`/games/recent`、`/games/random`、`/games/{game_id}`、`/games/{game_id}/platforms`、`/games/{game_id}/platforms/{platform_id}`、`.../screenshots`、`.../covers`。
`/games` 的过滤参数只有 `id`、`limit`(≤100)、`offset`、`platform`、`genre`、`group`、`title`（子串，≤128 字符，不区分大小写）、`format`(id/brief/normal)。
最接近的是 Silver 档才有的 `Product Codes` / `Identifiers`（`/games/{id}/platforms/{pid}` 的 `releases[].product_codes`），是**商品编号而非 ROM 哈希**。

### 6.4 媒体类型

**【事实】**
- `sample_cover` + `sample_screenshots`（normal 格式内嵌，含 `image`/`thumbnail_image`/`width`/`height`/`caption`）
- `/games/{id}/platforms/{pid}/covers` → **`cover_groups`，按国家/地区分组**（示例含 Germany / United Kingdom / Brazil），每张有 `scan_of`（`Front Cover`/`Back Cover`/`Media`）→ **区域封面区分度是各源里最好的**
- `/games/{id}/platforms/{pid}/screenshots` → 带 `caption`
- Promo Images 需 Bronze+；**原始全尺寸图需 Silver+**，低档只有压缩图

**【事实】中文别名**：`/games?format=normal` 的官方响应示例里带 `alternate_titles`，其中一条：
```json
{"description": "Simplified Chinese spelling", "title": "X档案"}
```
（即「X档案」）→ MobyGames 有**结构化的 `alternate_titles[].description` 标注中文别名**。
**【未确认】** AKAs 是 Silver（$499.99/月）才解锁的内容，低档位是否实际返回该字段。

### 6.5 授权条款

**【事实】** <https://www.mobygames.com/api/subscribe/> FAQ 原文：
> "**What are the requirements and usage limitations?**
> • Must include credit to MobyGames wherever the data is used (**"Data by MobyGames.com"**)
> • **Cannot compete directly with MobyGames**
> • **Cannot repackage or resell the data**
> • Cannot claim the data as your own"

> "**Can I store data on my server?** Yes. That's what we recommend so you can provide the lowest latency possible for your end product and users."

→ **缓存明确允许且被鼓励**；**署名强制**（字面要求 "Data by MobyGames.com"）；**禁止再打包/转售**；**禁止做直接竞品**。

---

## 7. GiantBomb API —— **当前已整体下线**

一手来源：<https://www.giantbomb.com/api/>

**【事实】** 官方页面原文：
> "The Giant Bomb API has long been a free resource for data on video games and the people that make them - and with the **move to become independent from Fandom, we had to rebuild our tech stack completely from scratch**."
> "We DID bring all the wiki's data with us - but **the mechanisms by which one could access them are not currently available, and they will change**."
> "**Not currently available are any of the APIs for:** - Games - Characters - Companies - Concepts - Locations - Objects - Releases - People - and so on and so forth."

页面版权行："Copyright 2026 **Jeffinitely LLC**. All rights reserved."（已脱离 Fandom，换主体运营）

**【事实】佐证**：旧文档路由已删除 ——
- `https://www.giantbomb.com/api/documentation/` → `{"message":"Route not found \"/api/documentation\""}`
- `https://www.giantbomb.com/api/list/` → `{"message":"Route not found \"/api/list\""}`

**【事实】重建方向**：官方指向开源重建项目 —— "If you navigate to `https://bombcast.com/wiki` you will find our GitHub for the Open Source Giant Bomb Wiki project; we are in need of PHP developers and some Javascript development, as well as **API developers looking to restore capability**."。联系 `support@giantbomb.com`。

**【未确认】** 历史文档中的「200 requests/resource/hour、1 request/second」等数值，当前官方站点已无该页面，**无法用一手来源验证**。

> **结论**：GiantBomb 在可预见的未来**不可纳入数据源设计**。如需跟进，观察 <https://www.giantbomb.com/api/> 与 <https://bombcast.com/wiki>。

---

## 8. No-Intro / Redump / TOSEC / MAME hash

### 8.1 ⚠️ No-Intro Dat-o-Matic 会自动永久封禁 IP（最重要的实操结论）

**【事实】实测**：用标准浏览器 UA 请求 `https://datomatic.no-intro.org/index.php?page=download`（**缺少必需的 `&s=<系统ID>` 参数**），**单次请求即触发永久 IP 封禁**，之后整站返回：
> "Something went wrong with your client or another client on your network: Wrong 'page' param in URL. Error id: 2861. **The ban won't be lifted until you contact me.** To remove the ban: shippa6@hotmail.com (please include the above error id)."

**【事实】robots.txt 与实际行为矛盾**：
```
# https://no-intro.org/robots.txt
User-agent: *
crawl-delay: 10

# https://datomatic.no-intro.org/robots.txt
User-agent: *
Disallow: 
Crawl-delay: 5
Request-rate: 1/5
Sitemap: http://datomatic.no-intro.org/sitemap.xml
```
robots.txt 名义上允许抓取（`Disallow:` 为空）并给出 5 秒节流，但**服务端逻辑对参数畸形零容忍并直接封禁**。**robots.txt 在此不能作为安全依据。**

> **结论：绝不要让程序直接抓取 datomatic。**

**【事实】官方获取流程**（<https://wiki.no-intro.org/index.php?title=Database_Navigation_Guide>）：
> "1. Go to the 'Download' section 2. Choose the system you want to download the dat for, **or choose 'Daily' to download a daily-generated pack of DATs** 3. Change the settings from the defaults, if you want. Click the download button and follow the on-screen instructions"

**【未确认】** 下载页是否有 CAPTCHA（因 IP 被封无法验证）。官方 Wiki **完全没有**记载速率限制、封禁政策或自动化规则。

**【事实】✅ 推荐替代：GitHub 每日镜像**（已实测下载验证）
<https://github.com/hugo19941994/auto-datfile-generator>（GitHub Actions 每日重建）：
> "WWW profiles to use in clrmamepro for the standard No-Intro and Redump sets. **Refreshes once every 24h automatically.**"

| URL | 内容 |
|---|---|
| `.../releases/latest/download/no-intro.zip` | **334 个真实 No-Intro DAT**，106 MB（实测下载解压确认） |
| `.../releases/latest/download/no-intro.xml` | clrmamepro WWW profile 索引 |
| `.../releases/latest/download/no-intro_parent-clone.xml` | Parent-Clone 版索引 |
| `.../releases/latest/download/redump.xml` | Redump profile |

走 GitHub Releases CDN，**无封禁风险、无速率问题、可直接 CI 集成**。
其他已验证存在的相关仓库：<https://github.com/unexpectedpanda/retool-clonelists-metadata>、<https://github.com/laromicas/datoso_seed_nointro>、<https://github.com/one-retro/datary>（Rust DAT 解析 crate）。

**【事实】No-Intro DAT 结构**（实测 + 官方 XSD <https://datomatic.no-intro.org/stuff/schema_nointro_datfile_v4.xsd>）：
- `<game>` 属性：`name`(必需)、`id`、`cloneof`、`cloneofid`
- `<rom>` **必需**：`name`、`size`、`crc`、`md5`、`sha1`
- `<rom>` **可选**：**`sha256`**、`status`、`serial`、`header`

实测条目：
```xml
<game name="007 - Everything or Nothing (Japan)" id="1468" cloneofid="1256">
    <description>007 - Everything or Nothing (Japan)</description>
    <rom name="007 - Everything or Nothing (Japan).gba" size="8388608"
         crc="caf2e99f" md5="55354d9e3bc9c1fa682b5110e5ed1544"
         sha1="6e4e9be9a07580ef267be9c2ea1bd0730b3be44a" serial="BJBJ"/>
</game>
```
`status` 实测取值 `{"verified", "baddump"}`。某 GBA DAT 共 3,533 条，1,791 条带 `cloneofid`、766 条带 `status`。
> **注意 `sha256` 的存在** —— 对去重与完整性校验很有价值。

**【未确认】** Standard 与 Parent-Clone 的官方正式定义（官方 Wiki 与论坛均无正式文档）。**【推断】** 从 XSD 与实测数据看：Standard DAT 已包含 `cloneofid`（父条目数字 ID），XSD 中另有 `cloneof`（父条目**名称**字符串），Parent-Clone 版本应是填充后者并组织为 clrmamepro 可识别的父子集合结构。

**【事实】许可与声明**：DAT header 内嵌两段官方声明：
> `<trademarks>` "Product names used in this file are registered trademarks of their respective owners, with which No-Intro is in no way associated or affiliated..."
> `<piracy>` "Piracy harms consumers as well as legitimate developers, publishers and retailers. **No-Intro does not help or encourage users to download or otherwise obtain any of the listed dumps.** No-Intro is not in the Console Scene and is not involved in any P2P sharing."

**未找到明示的数据许可证或再分发条款。** 但 header 中列有数十位贡献者的 `<author>` 署名，且 libretro（CC BY-SA 4.0）与多个 GitHub 镜像长期公开再分发未受阻，**【推断】** 构成事实上的容忍。

### 8.2 Redump —— 自动化最友好

一手来源：<http://redump.org/>、<http://redump.org/downloads/>、<http://redump.org/robots.txt>

**【事实】** `http://redump.org/datfile/{system}/` **直接返回 ZIP，无需注册、无需登录**：
```
$ curl -sI "http://redump.org/datfile/psx/"
HTTP/1.1 200 OK
Content-Disposition: attachment; filename="Sony - PlayStation - Datfile (10914) (2026-06-15 11-55-46).zip"
Content-Length: 4021187
Content-Type: application/zip
```

**【事实】** 全部 URL 前缀（从 `/downloads/` 页面提取）：

| 前缀 | 内容 |
|---|---|
| `/datfile/{sys}/` | Logiqx XML DAT（主要目标） |
| `/cues/{sys}/` | Cue sheets 打包 |
| `/gdi/{sys}/` | GDI 文件（Dreamcast/Naomi/Chihiro/Triforce） |
| `/sbi/{sys}/` | 子通道数据（PSX/PC/Mac 反拷贝） |
| `/dkeys/{sys}/`、`/keys/{sys}/` | 光盘密钥（PS3 等） |

系统短代码示例：`psx ps2 ps3 psp ss dc mcd pce pc-fx ngcd 3do cdi cd32 cdtv acd ajcd mac arch fmt x68k pc xbox xbox360 trf naomi naomi2 chihiro ks573 ...`（共 50+）。

**【事实】DAT 内容**（实测解析 10,914 条 PSX 记录）：标准 Logiqx XML，`DOCTYPE` 指向 `http://www.logiqx.com/Dats/datafile.dtd`。**cue 作为独立 `<rom>` 出现，每轨道各有完整三种哈希**：
```xml
<game name="Music - Music Creation for the PlayStation (Europe) (En,Fr,De,Es,It)">
    <category>Games</category>
    <rom name="....cue" size="306" crc="08b43a5a" md5="79aa7750..." sha1="56dc1f5d..."/>
    <rom name="... (Track 1).bin" size="323082480" crc="ef3e1282" md5="aa457b73..." sha1="e9bfae82..."/>
    <rom name="... (Track 2).bin" size="33868800" crc="3f9896b3" md5="441eeb42..." sha1="a55870ea..."/>
</game>
```

**【事实】⚠️ Redump 官方 DAT 不含 serial。** 程序化验证：全文 `'serial=' in s` → **False**（文件中 16 处 "serial" 全部来自游戏名 *Serial Experiments Lain*）。
> 若需要光盘 serial 做匹配，**用 libretro 的 `metadat/redump/` 版本**，而非 Redump 官方 DAT。

**【事实】robots.txt 全站放行**：
```
User-agent: *
Disallow:
```
空 `Disallow` = 全站放行，无 `Crawl-delay`、无 `Request-rate`。**这是全部对象中对自动化最友好的策略。**

**【未确认】** 未找到 Redump 官方的 ToS、速率限制明文声明、数据许可声明，也**没有官方 API**。DAT header 只有 `<author>redump.org</author>`。**【推断】** 仍应加入礼貌性节流。

### 8.3 TOSEC —— 唯一明确写 "All Rights Reserved"

一手来源：<https://www.tosecdev.org/downloads>、<https://www.tosecdev.org/robots.txt>

**【事实】定位**（libretro README 的评价）：
> "TOSEC data overlaps with and goes beyond other data sets (No-Intro, Redump), but has **lower precedence** in libretro and so generally serves as a **secondary stopgap**."

**【事实】规模**：2024-05-17 版 4,245 个 DAT / 1,033,614 sets / 1,294,449 ROMs / 10.35 TB；最新 2025-03-13 版 4,743 个 DAT。分 TOSEC-Main(2,830) / TOSEC-ISO(283) / TOSEC-PIX(1,132)。

**【事实】robots.txt** 是 Joomla 默认模板，仅 `Disallow` 后台目录，**未屏蔽 `/downloads`**，无 Crawl-delay。

**【事实】DAT 字段**（libretro 转换版实测）：含 crc/md5/sha1，**确认无 serial**。语义信息编码在**文件名**里（命名约定 `Game (Year)(Publisher)(Region)[flags]`，`[a]`=alternate、`[cr]`=cracked），解析需自行实现。

**【事实】许可**：站内唯一许可链接指向 GPL-2.0，但那是 **Joomla CMS 自身**的许可。页脚原文：
> "Copyright © 2026 TOSEC Project Homepage. **All Rights Reserved.** Designed by JoomlArt.com. Joomla! is Free Software released under the GNU General Public License."

**这是全部对象中唯一明确写出 "All Rights Reserved" 的。** 但 libretro 在 CC BY-SA 4.0 仓库中长期再分发 TOSEC 转换数据。**【未确认】** 是否存在正式授权或豁免。**【未确认】** 是否有官方 API 或 GitHub 镜像。

### 8.4 MAME hash / Software List XML —— ⭐ 许可最干净

一手来源：<https://github.com/mamedev/mame/tree/master/hash>、<https://github.com/mamedev/mame/blob/master/COPYING>、<https://github.com/mamedev/mame/blob/master/hash/softwarelist.dtd>

**【事实】许可（本次调研最有价值的发现之一）**：MAME 整体是 GPL-2.0，但 `hash/` 目录被**单独献给公有领域**。`COPYING` 原文：
> "MAME as a whole is made available under the terms of the GNU General Public License. Individual source files may be made available under less restrictive licenses, as noted in their respective header comments."
> **"The contents of the hash directory are dedicated to the public domain as described in docs/legal/CC0."**

且**每个 hash XML 文件自带 SPDX 头**（实测 `nes.xml`/`snes.xml`/`megadriv.xml`/`gameboy.xml`/`gba.xml`/`psx.xml` 全部一致）：
```xml
<?xml version="1.0"?>
<!DOCTYPE softwarelist SYSTEM "softwarelist.dtd">
<!--
license:CC0-1.0
-->
```

> **这是唯一可以无条件商业使用、无署名义务、无 ShareAlike 传染的哈希源。**

**【事实】范围界定**：`hash/` 存放的是**家用系统的 software list**（卡带/软盘/光盘），主要为 `.xml`。**街机 ROM 的哈希不在此处**，而是硬编码在 `src/mame/**` 的 C++ 驱动源码中；街机需改用 MAME 官方 `-listxml` 输出或 libretro 的 `metadat/mame/`。

**【事实】Schema**（`softwarelist.dtd` 关键部分）：
```dtd
<!ELEMENT software (description, year, publisher, notes?, info*, sharedfeat*, part*)>
  <!ATTLIST software name CDATA #REQUIRED>
  <!ATTLIST software cloneof CDATA #IMPLIED>
  <!ATTLIST software supported (yes|partial|no) "yes">
  <!ELEMENT rom EMPTY>
    <!ATTLIST rom name CDATA #IMPLIED>
    <!ATTLIST rom size CDATA #IMPLIED>
    <!ATTLIST rom crc CDATA #IMPLIED>          ← 仅 CRC
    <!ATTLIST rom sha1 CDATA #IMPLIED>         ← 仅 SHA1
    <!ATTLIST rom status (baddump|nodump|good) "good">
  <!ELEMENT disk EMPTY>
    <!ATTLIST disk sha1 CDATA #IMPLIED>        ← 仅 SHA1
```
> **DTD 权威确认：`<rom>` 只有 `crc` 和 `sha1`，没有 `md5`、没有 `serial`。`<disk>` 只有 `sha1`。**

**【事实】无 API**，官方发布见 <https://www.mamedev.org/release.html>（源码包内含 hash 目录）。

---

## 9. 中文/华语区数据源

> 本章的覆盖率数字大部分来自**实时 API 调用**（2026-08-30），而非文档陈述。

### 9.1 Bangumi (bgm.tv) —— ★ 中文游戏元数据的最佳来源

一手来源：
- OpenAPI 源：<https://raw.githubusercontent.com/bangumi/api/master/open-api/v0.yaml>（OpenAPI 3.0.2）
- 交互文档：<https://bangumi.github.io/api/>
- User-Agent 要求：<https://github.com/bangumi/api/blob/master/docs-raw/user%20agent.md>
- OAuth 流程：<https://github.com/bangumi/api/blob/master/docs-raw/How-to-Auth.md>
- 版权声明：<https://bangumi.tv/about/copyright>
- 离线 dump：<https://github.com/bangumi/Archive>

#### 注册 / 认证 / 收费

**【事实】只读 API 完全免注册、免 token。** 实测 `GET /v0/subjects/{id}`、`GET /v0/subjects?type=4`、`POST /v0/search/subjects` 在**无 `Authorization` 头**下均返回 200。

v0.yaml `securitySchemes` 原文：
- `OptionalHTTPBearer`：「不强制要求用户认证，但是可能看不到某些敏感内容内容（如 NSFW 或者仅用户自己可见的收藏）」
- `HTTPBearer`：OAuth2 authorizationCode，scopes 仅 `write:collection`、`write:indices`

**【事实】OAuth2 流程**（仅写操作需要）：`GET https://bgm.tv/oauth/authorize` → `POST https://bgm.tv/oauth/access_token`；「`code` 的有效期为 60 秒」；access_token `expires_in: 604800`（**7 天**）；「新的 API(`/v0/`) 不再允许使用 query string 传递 Access Token」。

**【事实】免费**，一手来源中无任何收费条款。

**【未确认】** <https://next.bgm.tv/demo/access-token> 返回 **HTTP 403**（需登录），个人长期令牌的申请细节无法从匿名一手来源确认。

#### User-Agent 要求

**【事实】** 官方原文（<https://github.com/bangumi/api/blob/master/docs-raw/user%20agent.md>）：
> 非浏览器的 API 使用者请指定一个带有**开发者个人 ID** 和**应用名称**的 User Agent。
> 如果你的应用需要进行分发…请附上**版本号**。如果你的应用是一个开源项目，请在 User Agent 附上项目主页。
> **各种请求库的默认 UA 可能会被禁用。**
> **请不要使用类似于 `database`，`Bangumi/1.0`，`Bangumi/1.3.13.0` 的 User Agent。**

推荐格式：`czy0729/Bangumi/6.4.0 (Android) (http://github.com/czy0729/Bangumi)`

**【事实】实测**：`curl/8.x`、`python-requests/2.31.0`、`Bangumi/1.0`（明文禁止的格式）三种 UA **全部返回 200** —— 当前是「建议 + 可能被禁用」，未观察到硬性拦截。**【推断】** 生产环境仍应遵守。

#### 速率限制

**【事实】未见任何明文速率限制。** v0.yaml 全文 grep `429` / `too many` / `频率` / `限流` → **0 命中**（responses 只定义 400/401/404/422）。实测响应头无 `RateLimit-*` / `Retry-After`，只有 `server: cloudflare`、`cache-control: public, max-age=3600`。
**【推断】** 前置 Cloudflare 可能有未公开的 WAF 策略。

#### 是否支持 ROM 哈希查询 —— **不支持**

**【事实】** v0.yaml 全文 grep `hash` / `md5` / `crc` / `rom` / `checksum` / `sha1` → **0 命中**。46 个 path 中无任何哈希端点。

#### 覆盖度评估（★ 重点，全部为实测）

**【事实】`GET /v0/subjects?type=4&platform=<平台>` 返回的 `total`（2026-08-30）：**

| 平台 | 条目数 | 平台 | 条目数 | 平台 | 条目数 |
|---|---:|---|---:|---|---:|
| **全部 type=4** | **87,188** | PS2 | 1,588 | FC | 676 |
| PC | 59,964 | PSP | 1,177 | Wii | 537 |
| Android | 7,456 | PS | 1,003 | SFC | 525 |
| iOS | 7,323 | Xbox 360 | 845 | GBA | 512 |
| Nintendo Switch | 5,304 | NDS | 836 | MD | 300 |
| PS4 | 4,441 | 3DS | 733 | 街机 | 264 |
| Xbox One | 3,218 | | | GB | 231 |
| PS5 | 2,862 | | | NGC | 193 |
| XBOX Series X/S | 2,387 | | | N64 | 98 |
| | | | | Dreamcast | 38 |

**【事实】欧美主机游戏抽样**（`POST /v0/search/subjects`，`filter.type=[4]`，均命中且有 `name_cn`）：

| 查询 | 命中条目 | name_cn |
|---|---|---|
| Super Mario Odyssey | スーパーマリオ オデッセイ | 超级马力欧 奥德赛 |
| The Legend of Zelda: BotW | ゼルダの伝説 ブレス オブ ザ ワイルド | 塞尔达传说 旷野之息 |
| Sonic the Hedgehog 2 | Sonic the Hedgehog 2 | 刺猬索尼克2 |
| Super Metroid | スーパーメトロイド | 超级密特罗德 |
| Chrono Trigger | クロノ・トリガー | 时空之钥 |
| Halo: Combat Evolved | Halo: Combat Evolved | 光环：战斗进化 |
| GTA: Vice City | Grand Theft Auto: Vice City | 侠盗猎车手 罪恶都市 |
| Ristar | リスター・ザ・シューティングスター | 外星王子 |
| Gunstar Heroes | ガンスターヒーローズ | 火枪英雄 |
| Wonder Boy III | モンスターワールドII ドラゴンの罠 | 怪物世界2 龙之陷阱 |
| ToeJam & Earl | ToeJam & Earl | 外星双傻 |
| Bomberman '94 | ボンバーマン'94 | 炸弹人94 |

**【事实】中文简介的实际比例**（对 summary 统计 CJK 字符占比）：
`超级马力欧 奥德赛` 86% ／ `塞尔达传说 旷野之息` 76% ／ `光环：战斗进化` 79% ／ `时空之钥` 72% ／ `超级密特罗德` 31%（部分条目是英文导入）

**【事实】两个必须注意的失败模式**（实测）：
- `Alex Kidd in Miracle World` → 返回 DX 复刻版，且 `name_cn` **为空**
- `Solar Jetman` → **错配到 "Solar 2"** —— 搜索是模糊最近邻，**不做精确匹配**

**结论**：
- **【事实】Bangumi 不是只做日系 galgame** —— 欧美主机游戏有条目、有中文译名、有中文简介。
- **【事实】但老平台深度浅**：SFC 525 条（实际商业作品 1,700+）、N64 98 条（实际约 390）、Dreamcast 38 条（实际约 600）、PCE 27 条。
- **【推断】** 定位应为**「中文名与中文简介的高质量补全层」**，而非「全量 romset 覆盖层」。且因为没有哈希/精确匹配，必须做**匹配置信度校验**（平台 + 年份 + 名称相似度三重校验），否则会引入 `Solar Jetman → Solar 2` 这类错配。

#### 字段与平台过滤

**【事实】** 两个同名但含义不同的 `platform` —— **这是最容易踩的坑**：
- **查询参数** `platform` = 主机平台（`PS2`/`SFC`/`NDS`…），仅 `GET /v0/subjects` 支持（`POST /v0/search/subjects` 的 filter **没有** platform 字段，但有 `meta_tags`）
- **返回体** `Subject.platform` = 条目**分类**（游戏/软件/DLC/Demo/桌游），dump 中是数字码 `4001=游戏 / 4002=软件 / 4003=DLC / 4004=Demo / 4005=桌游`（见 <https://github.com/bangumi/common/blob/master/subject_platforms.yml>）
- 主机平台真正存放在 `infobox` 的 `平台` 键 与 `meta_tags`

**【事实】实测 `meta_tags` 可作为 search 的平台过滤器**（`filter.meta_tags=["MD"]` 有效），但注意 Switch 的 tag 是 `NS` 不是 `Switch`（后者返回 0），且 search 的 `total` **上限为 1000**。

**【事实】实测字段**（subject 2113 = 塞尔达传说 时之笛）：
```
name: ゼルダの伝説 時のオカリナ     name_cn: 塞尔达传说 -时光之笛-
date: 1998-11-21    rating.score: 9.1 (613人)    meta_tags: ['N64','NGC','游戏','AAVG']
images: grid(100px) / small(200px) / common(400px) / medium(800px) / large(原图)
        例：https://lain.bgm.tv/r/400/pic/cover/l/01/93/2113_gOfF7.jpg
infobox: 中文名 / 别名[The Legend of Zelda: Ocarina of Time, 薩爾達傳說 時之笛, 塞尔达传说 时之笛]
         / 平台[NGC, N64] / 游戏类型 AAVG / 发行日期 / 其他发行日期[北美/欧洲/中国大陆]
         / 发行 / 游戏开发商 / 制作人 / 导演 / 音乐 / website
```
> `infobox` 的 `别名` 常同时含**英文原名 + 繁中名 + 简中别名** —— 对跨库匹配价值极高。

#### 授权与缓存条款

**【事实】** <https://bangumi.tv/about/copyright> 原文：
> 「Bangumi 番组计划中的条目信息（包括但不限于封面、内容介绍、章节信息）、角色信息均由用户提供，遵循 **Creative Commons BY-SA License** 协议，其版权归创作者所有。」
> 「对于已有版权的作品遵照 **Fair use** 原则处理，并标注来源。」

站点 logo/样式「可用于非商业用途，不可用于商业目的」。

**【事实】robots.txt**（<https://bgm.tv/robots.txt>）：`Disallow: /pic/ /img/ /js/` —— 但**封面实际托管在独立域 `lain.bgm.tv`**，不受该 robots 约束。
**【事实】图片 CDN 实测**：`https://lain.bgm.tv/r/400/pic/cover/...` 无防盗链（任意 Referer 均 200），`access-control-allow-origin: *`，`cache-control: max-age=691200, immutable`。

> **【推断】** 条目文本（name_cn / summary / infobox / tags）→ CC BY-SA，可缓存可再分发但需署名 + 相同方式共享。封面名义上也在 CC BY-SA 表述内，**但同页承认「对已有版权的作品遵照 Fair use 处理」—— 封面本身是发行商版权物，Bangumi 只是主张合理使用，CC BY-SA 无法为第三方再分发背书**。建议本地缓存自用，不做公开再分发。

#### ★ 离线 dump：bangumi/Archive（对离线优先架构的关键）

**【事实】** <https://github.com/bangumi/Archive>，README 原文：
> 「定期导出 wiki 数据方便一些不需要实时数据的场景，顺便希望减少一些爬虫。」
> 「**每周三凌晨五点(GMT+8)更新**」「导出的数据可以在 releases 下载」

| 项 | 实测值 |
|---|---|
| 最新地址索引 | <https://raw.githubusercontent.com/bangumi/Archive/master/aux/latest.json> |
| Release tag | `archive`（固定），故 `releases/latest` 恒指向它 |
| 最新包 | `dump-2026-08-25.210336Z.zip`，**434,740,633 bytes**，带 sha256 |
| 下载 URL | `https://github.com/bangumi/Archive/releases/download/archive/dump-<日期>.zip` |
| 格式 | **ZIP 容器，内含 9 个 `.jsonlines`** |

**【事实】ZIP 内容清单：**

| 文件 | 压缩 | 解压后 |
|---|---:|---:|
| `subject.jsonlines` | 281 MB | **959.9 MB** |
| `episode.jsonlines` | 66 MB | 335.5 MB |
| `character.jsonlines` | 44 MB | 160.8 MB |
| `subject-persons.jsonlines` | 13 MB | 151.7 MB |
| `subject-relations.jsonlines` | 6 MB | 73.9 MB |
| `person.jsonlines` | 19 MB | 71.0 MB |
| `subject-characters.jsonlines` | 2.3 MB | 27.4 MB |
| `person-characters.jsonlines` | 2.2 MB | 23.4 MB |
| `person-relations.jsonlines` | 0.8 MB | 14.2 MB |

**【事实】subject 记录实际结构**（实测 id=4）：
```json
{"id":4,"type":4,"name":"メタルスラッグ7","name_cn":"合金弹头7",
 "infobox":"{{Infobox Game\r\n|中文名= 合金弹头7\r\n|别名={\r\n[Metal Slug 7]\r\n}\r\n|平台= NDS\r\n|游戏类型= ACT\r\n|发行日期= 2008-07-17\r\n...}}",
 "platform":4001,"summary":"　　以细腻的画风…","nsfw":false,
 "tags":[{"name":"NDS","count":60},…],"meta_tags":["ACT","NDS","游戏"],
 "score":6.9,"score_details":{…},"rank":4354,"date":"2008-07-17",
 "favorite":{"wish":32,"done":240,"doing":21,"on_hold":14,"dropped":14},"series":false}
```

> **⚠️【事实】关键限制：dump 中没有任何图片字段。** 没有 `images`、没有封面 URL。README 明说导出的是「主键和原始 wiki 内容」。封面必须另行经 `GET /v0/subjects/{id}/image` 或 `/v0/subjects/{id}` 在线补齐。

**【事实】`infobox` 是原始 wiki 字符串，需要解析器。** 官方提供：语法规范 <https://github.com/bangumi/wiki-syntax-spec>；解析器 <https://github.com/bangumi/wiki-parser-go> / `wiki-parser-py`；常量表 <https://github.com/bangumi/common>。

**【事实】** Archive 仓库**无 LICENSE 文件**（tarball 内容仅 `README.md` + `.github/workflows/7z.yml`）。**【推断】** 许可回落到站点版权声明的 CC BY-SA。

### 9.2 VNDB —— 范围极窄，仅对 galgame 有用

一手来源：<https://api.vndb.org/kana>、<https://vndb.org/d2>、<https://vndb.org/d14>、<https://vndb.org/d17>、<https://vndb.org/robots.txt>

**【事实】** 旧 TCP API 文档 <https://vndb.org/d11> **已 301 永久重定向到 <https://api.vndb.org/kana/>**。

**【事实】只读免注册。** 实测 `POST /kana/vn` 无任何 Header 即返回数据。Token（仅写操作/私有列表）在 <https://vndb.org/u/tokens> 生成，用 `Authorization: Token <token>`。

**【事实】收费与保证**（原文）：
> "This service is **free for non-commercial use**. The API is provided on a best-effort basis, no guarantees are made about the stability or applicability of this service."

> ⚠️ **「仅非商业免费」** —— 商业用途需另行沟通（`contact@vndb.org`）。

**【事实】速率限制**（原文逐字核实）：
> "The server will allow up to **200 requests per 5 minutes** and up to **1 second of execution time per minute**. Requests taking longer than **3 seconds** will be aborted."

超限返回 HTTP 429。

**【事实】字段**（`POST /vn` 实测）：
```
id, title(罗马字主标题), alttitle(原文标题), olang(原语言)
titles[]: { lang, title, latin, official(bool), main(bool) }
aliases[], languages[], platforms[], released, devstatus
description, image{id,url,dims,sexual,violence,thumbnail}, screenshots[]
developers[], tags[], staff[], va[], relations[], editions[], extlinks[]
length, rating, votecount, average
```
实测返回样例（Clannad, v4）：
```json
"titles":[{"lang":"en","official":true,"title":"CLANNAD"},
          {"lang":"ja","official":true,"title":"CLANNAD"},
          {"lang":"ko","official":true,"title":"클라나드"},
          {"lang":"zh-Hans","official":true,"title":"CLANNAD"}]
```
**【事实】** `https://api.vndb.org/kana/schema` 实测：48 个 platform 枚举、57 个 language 枚举，中文三项 `zh` / `zh-Hans` / `zh-Hant`。

**【事实】覆盖度实测（2026-08-30）：**

| 查询 | count |
|---|---:|
| 全部 VN | **65,933** |
| `lang = zh-Hans` | **5,737**（8.7%） |
| `lang = zh-Hant` | **2,225**（3.4%） |
| `lang = zh`（旧码） | 0 |

**【事实】对主流主机游戏覆盖 ≈ 0**（我本人实测）：
```
"Super Mario Odyssey"    → {"more":false,"results":[]}
"Chrono Trigger"         → {"more":false,"results":[]}
"Sonic the Hedgehog"     → 仅 v43470 "The Murder of Sonic the Hedgehog"
```

**【事实】收录范围**（<https://vndb.org/d2>）：
> "Choices are the only allowed (but optional) form of interactivity. There are no other gameplay elements..." / "at least **99% of the title should be made of pure reading**"
> VN/Game 混合：「At least 50% of the game should be made of pure, VN-style reading」
> 老式 AVG 前身有祖父条款，但 "new additions in this category are no longer accepted"

**【事实】不支持 ROM 哈希**，但 `/release` 端点有实体版标识符（实测 r36）：`gtin`（EAN/JAN/UPC 条码）、`catalog`（品番）、`media`。
**【推断】** 对**实体卡带/光盘**的匹配比 ROM 哈希更实用（日版卡带盒背面有 JAN 码），但 VNDB 只覆盖 VN，实用面窄。

**【事实】离线 dump**（<https://vndb.org/d14>，每日 ~08:00 UTC，<https://dl.vndb.org/dump/>）：

| 文件 | 实测大小 | 许可（原文） |
|---|---:|---|
| `vndb-db-latest.tar.zst` | **187,546,617 B** | "See our Data License or the included README.txt" |
| `vndb-tags-latest.json.gz` | 354 KB | "Open Database License + Database Contents License" |
| `vndb-votes-latest.gz` | 13.8 MB | "Open Database License" |
| `rsync://dl.vndb.org/vndb-img/` | 角色 ~3.5 GB / 封面 ~9 GB / 截图 ~30 GB | "**Open Database License for the collection, use of individual images is considered \"fair use\"**" |

文档警告："**This database dump is NOT considered a stable API**"

**【事实】授权条款**（<https://vndb.org/d17#4>）：
> "All information on VNDB is made available under the **Open Database License**"（+ Database Contents License）
> "**Images, visual novel descriptions and character descriptions are gathered from various online sources and may be subject to separate license conditions.**"

**【事实】robots.txt 的站方明确表态**（值得一读的一手来源）：
```
# Please do not crawl VNDB if you can avoid it, we don't have a lot server resources to spare.
# If you're interested in fetching real-time information, we have an API: https://api.vndb.org/kana
# If you're interested in bulk data, we have various database dumps: https://vndb.org/d14
# If you're interested in custom querying, we have an interface for that: https://query.vndb.org/
Crawl-delay: 10
```
之后是一长串 AI 爬虫黑名单（含 `ClaudeBot`、`Claude-User`、`anthropic-ai`、`CCBot`、`Bytespider`）。**【推断】** 站方态度明确：**用 API 和 dump，别爬页面**。

### 9.3 Steam 官方端点 —— PC 游戏中文元数据的最佳来源

#### Storefront `appdetails` —— **可用，能拿到中文名与中文简介**

**【事实】** `https://store.steampowered.com/api/appdetails?appids=<id>&l=schinese`，实测（2026-08-30）：

| appid | `name` | `short_description`（截断） |
|---|---|---|
| 1091500 | **赛博朋克 2077** | 《赛博朋克 2077》是一款开放世界动作冒险 RPG 游戏… |
| 292030 | **《巫师 3：狂猎 - 完全版》** | 您是利维亚的杰洛特，收钱办事的怪物杀手… |
| 570 | Dota 2 | 每一天全球有数百万玩家化为一百余名Dota英雄展开大战… |
| 814380 | `Sekiro™: Shadows Die Twice`（**英文**） | 进入由打造了《黑暗之魂》系列的知名开发商FromSoftware… |
| 1196590 | `Resident Evil Village`（**英文**） | 在《Resident Evil》系列最新游戏中展开令人毛骨悚然的绝命拚搏… |
| 620 | `Portal 2`（**英文**） | "终身测试计划"现已升级… |

**结论【事实】**：
- `short_description` **几乎总是中文**（Valve 强制本地化商店描述）
- `name` **仅当 Valve 商店页有中文标题时才是中文** —— 覆盖不完整
- 另可拿到 `header_image`、`developers`、`publishers`、`release_date.date`（本地化为「2020 年 12 月 10 日」）、`supported_languages`
- 支持 `&filters=basic` 减小响应体

**【事实】非官方性质**：该端点**不在** Valve 官方文档 <https://partner.steamgames.com/doc/webapi/> 中，无版本号、无 SLA、无文档化限流。
**【未确认】** 实际限流数值 —— Valve 从未文档化该端点。
**【事实】** <https://store.steampowered.com/robots.txt> 中 **`/api/` 未被 Disallow**。

#### `ISteamApps/GetAppList` —— **已下线**

**【事实】** 实测 `https://api.steampowered.com/ISteamApps/GetAppList/v2/` 返回 **HTTP 404**：
```
Method 'GetAppList' not found in interface 'ISteamApps'
```
`v1` / `v0002` 同样 404。`ISteamWebAPIUtil/GetSupportedAPIList/v1` 显示 `ISteamApps` 现在仅剩 `GetSDRConfig/v1`、`GetServersAtAddress/v1`、`UpToDateCheck/v1`。

**【事实】** Valve 官方文档 <https://partner.steamgames.com/doc/webapi/ISteamApps> 的标注：
> "**Deprecated - this API can no longer scale to the number of items available on Steam.**"（建议改用 `IStoreService/GetAppList`）

`IStoreService/GetAppList` 需 API key（无 key 返回 Forbidden）。

**【推断】替代方案**：appid 列表可从 **Wikidata 的 124,464 个 `P1733`（Steam application ID）反查**，绕开已下线的 GetAppList。

#### 条款

**【事实】** <https://steamcommunity.com/dev/apiterms>：
> "You are limited to **one hundred thousand (100,000) calls to the Steam Web API per day**."
> "You will only retrieve Steam Data about a Steam end user as requested by the end user"

**【推断】注意**：该条款约束的是 `api.steampowered.com` 的 Steam Web API；`store.steampowered.com/api/appdetails` 是商店前端端点，**不在这份条款的明确范围内，也没有自己的条款页 —— 法律状态灰色**。

### 9.4 其他中文站点的可用性评估 —— 均建议排除

| 站点 | 公开 API | robots.txt | 条款 | 结论 |
|---|---|---|---|---|
| **GameFAQs / GameSpot** | **【未确认】** | **【未确认】** | **【未确认】**（Fandom 条款页返回 HTTP 402） | **【推断】** 历史上以严格反爬著称（Cloudflare + 无公开 API）。**直接排除，不要投入工程量。** |
| **机核 gcores.com** | **【事实】无**（`/api/v1/games`、`/gapi/v1/games` 均 404） | 【事实】Disallow `/search /terms /account/ …`；`Sogouspider: Disallow: /` | **【事实】** 用户协议 7.3：「**不得在任何平台被直接或间接发布、使用、出于发布或使用目的的改写或再发行**」 | **排除** —— 无库 + 条款明禁再发布 |
| **游民星空** | 【事实】`open.gamersky.com` 无法连接 | **【事实】游戏库 `ku.gamersky.com` 的 robots.txt 明确 `User-agent: ClaudeBot / Disallow: /`**（同时封禁 `Meta-ExternalAgent`、`dotbot`） | — | **排除** |
| **3DM** | 【事实】`open.3dmgame.com` → 404 | 仅挡 `/runtime/ /config/ /tests/ …` | — | 无 API、无结构化库 |
| **A9VG** | 无 | `Disallow:`（空 = 全放行） | — | 论坛为主，无游戏库 |
| **巴哈姆特 acg.gamer.com.tw** | 【事实】`open.gamer.com.tw` 无法连接 | 【事实】`Allow: /` + `Sitemap:` | 无许可声明 | **【推断】** 繁中游戏元数据最全的站点之一，理论可爬但**无 API、无许可、需繁转简**。只作最后手段，风险自负 |
| **豆瓣游戏** | **【事实】** `https://api.douban.com/v2/game/10001` → nginx **404** | 【事实】未 Disallow `/game/`，但明确 Disallow `/search`、`/j/`（JSON 内部接口） | **【未确认】**（`developers.douban.com` 不可达，官方关闭公告无法取证） | **【推断】** 官方游戏 API 事实上已不可用。**排除** |

---

## 10. Wikidata / 中文维基百科

一手来源：
- <https://query.wikidata.org/sparql>
- <https://www.mediawiki.org/wiki/Wikidata_Query_Service/User_Manual>
- <https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy>
- <https://www.mediawiki.org/wiki/Wikimedia_APIs/Rate_limits>
- <https://www.wikidata.org/wiki/Wikidata:Licensing>
- <https://commons.wikimedia.org/wiki/Commons:Licensing>

### 10.1 使用限制

**【事实】WDQS**（User Manual 原文）：
> "There is a hard query deadline configured which is set to **60 seconds**."
> "Currently access to the service is limited to **5 parallel queries per IP**."
> "One client (user agent + IP) is allowed **60 seconds of processing time each 60 seconds**"
> "One client is allowed **30 error queries per minute**"（超限 HTTP 429 + `Retry-After`）
> "Clients who don't comply with the User-Agent policy **may be blocked completely**"

**【事实】实测印证**：两个带 `FILTER(LANG(?l)="zh")` 的**全库** COUNT 查询（175k 实体规模）确实超时（一次 60s timeout，一次 HTTP 504）；把范围收窄到单平台（1k–7k 实体）后全部成功。

**【事实】UA 政策**（Wikimedia Foundation User-Agent Policy 原文）：
> 格式：`User-Agent: CoolBot/0.0 (https://example.org/coolbot/; coolbot@example.org) generic-library/0.0`
> "Scripts should use an informative User-Agent string with contact information, **or they may be blocked without notice**."
> "**Do not copy a browser's user agent for your bot**, as bot-like behavior with a browser's user agent will be assumed malicious."

**【事实】MediaWiki API 速率限制**（按每分钟计，<https://www.mediawiki.org/wiki/Wikimedia_APIs/Rate_limits>）：

| 客户端类型 | 限额 |
|---|---:|
| **仅 IP、无其它标识** | **10 req/min** |
| 浏览器发起的匿名请求 | 200 req/min |
| **带合规 UA 的匿名 bot** | **200 req/min** |
| 已认证资深编辑 | 2,000 req/min |

> ⚠️ **匿名 + 无标识 UA 只有 10 req/min；带合规 UA 提升到 200 req/min —— 差 20 倍。**

### 10.2 属性号核实（★ 常见的记忆错误）

**【事实】** 通过 `wbsearchentities?type=property` 逐一核实：

| 属性 | 编号 | 备注 |
|---|---|---|
| instance of / platform / developer / publisher | P31 / P400 / P178 / P123 | ✅ |
| publication date / genre / game mode / image | P577 / P136 / P404 / P18 | ✅ |
| GOG application ID / Steam application ID / Metacritic ID | P2725 / P1733 / P1712 | ✅（另有 P12054 = Metacritic game ID，更专用） |
| **IGDB game ID** | **P5794**（标签："Internet Game Database game ID"） | ⚠️ 不是 P9968；另有 P9043 = numeric game ID |
| **MobyGames game ID** | **P11688** | ⚠️ **P1933 是 "MobyGames game ID (former scheme)"，已弃用** |
| **TheGamesDB game ID** | **P7622** | ⚠️ 不是 P5085 |
| **Bangumi subject ID** | **P5732** | ★ 可直接桥接到 Bangumi |
| GameFAQs game ID / 豆瓣游戏 ID | P4769 / P6444 | |

其他：`P5795` IGDB platform ID、`P9650` IGDB company ID、`P5868` MobyGames platform ID、`P7623/P7634/P7642` TheGamesDB platform/developer/publisher ID。

**【事实】外部 ID 覆盖量**（实测，`?g wdt:P31 wd:Q7889` 共 **175,739** 条）：

| 属性 | 条目数 | 占比 |
|---|---:|---:|
| **P5794 IGDB** | **142,011** | **80.8%** |
| P1733 Steam | 124,464 | 70.8% |
| P11688 MobyGames | 64,594 | 36.8% |
| P4769 GameFAQs | 17,256 | 9.8% |
| P5732 Bangumi | 5,410 | 3.1% |
| P7622 TheGamesDB | 1,442 | **0.8%（几乎不可用）** |

### 10.3 中文标签覆盖率（★ 实测 SPARQL COUNT）

**【事实】** `wdt:P31 wd:Q7889` + `wdt:P400 <平台>`，中文标签 = `rdfs:label` 且 `LANG="zh"`：

| 平台 | QID | 条目总数 | 有中文标签 | 覆盖率 |
|---|---|---:|---:|---:|
| PlayStation 2 | Q10680 | 3,151 | 1,308 | **41.5%** |
| FC / NES | Q172742 | 1,205 | 461 | **38.3%** |
| SFC / SNES | Q183259 | 1,424 | 449 | **31.5%** |
| Nintendo DS | Q170323 | 1,838 | 577 | **31.4%** |
| GBA | Q188642 | 1,061 | 331 | **31.2%** |
| Mega Drive | Q10676 | 1,028 | 301 | **29.3%** |
| PlayStation | Q10677 | 2,140 | 609 | **28.5%** |
| Nintendo Switch | Q19610114 | 7,148 | 1,966 | **27.5%** |

**【事实】重要发现：`"zh"` 是唯一实际使用的中文标签语言码。** FC 平台下 `LANG(?l)="zh"` 得 461，`STRSTARTS(LANG(?l),"zh")`（涵盖 zh-hans/zh-hant/zh-cn/zh-tw/zh-hk）**同样得 461** → 游戏条目基本不用中文变体码，只需 filter `"zh"`。

**【事实】FC 平台其他维度深挖**（共 1,205 条）：

| 维度 | 数量 | 占比 |
|---|---:|---:|
| 有中文标签 `rdfs:label@zh` | 461 | 38.3% |
| 有中文别名 `skos:altLabel@zh` | 73 | 6.1% |
| 有中文维基条目（zhwiki sitelink） | 355 | 29.5% |
| **有图片 `wdt:P18`** | **41** | **3.4%** ⚠️ |
| 有 IGDB ID | 1,067 | 88.5% |
| 有 MobyGames ID | 1,020 | 84.6% |
| 有 TheGamesDB ID | 135 | 11.2% |

> **结论**：Wikidata 中文标签覆盖率 **~27–42%**，低于 Bangumi 的中文名密度；但 **Wikidata 是 CC0，法律上最干净，且外部 ID 桥接能力最强**。**图片完全不可用（3.4%）。**

### 10.4 可用的 SPARQL 模板（已实测跑通）

```sparql
# 端点：https://query.wikidata.org/sparql
# 必须带合规 UA，例如：
#   MyGameApp/1.0 (https://example.org/mygameapp; you@example.org) curl/8

SELECT ?game ?zhLabel ?zhAlias ?enLabel ?date ?igdb ?moby ?bgm ?steam WHERE {
  ?game wdt:P31 wd:Q7889 ;          # instance of: video game
        wdt:P400 wd:Q183259 .        # platform: SFC / SNES（换 QID 即换平台）
  OPTIONAL { ?game rdfs:label    ?zhLabel FILTER(LANG(?zhLabel)="zh") }
  OPTIONAL { ?game skos:altLabel ?zhAlias FILTER(LANG(?zhAlias)="zh") }
  OPTIONAL { ?game rdfs:label    ?enLabel FILTER(LANG(?enLabel)="en") }
  OPTIONAL { ?game wdt:P577   ?date  }      # publication date
  OPTIONAL { ?game wdt:P5794  ?igdb  }      # IGDB game ID
  OPTIONAL { ?game wdt:P11688 ?moby  }      # MobyGames game ID
  OPTIONAL { ?game wdt:P5732  ?bgm   }      # Bangumi subject ID
  OPTIONAL { ?game wdt:P1733  ?steam }      # Steam application ID
}
LIMIT 500
```
实测返回样例：`Q100096 / 洛克人X2 / Mega Man X2 / 1994-12-16 / mega-man-x2 / 6580`

**统计用 COUNT 模板**（**勿对全库跑标签 filter，必超时**）：
```sparql
SELECT ?plat (COUNT(DISTINCT ?g) AS ?total) (COUNT(DISTINCT ?gz) AS ?zh) WHERE {
  VALUES ?plat { wd:Q172742 wd:Q183259 wd:Q10676 wd:Q10677
                 wd:Q10680 wd:Q188642 wd:Q170323 wd:Q19610114 }
  ?g wdt:P31 wd:Q7889 ; wdt:P400 ?plat .
  OPTIONAL { ?g rdfs:label ?zl . FILTER(LANG(?zl)="zh") BIND(?g AS ?gz) }
} GROUP BY ?plat
```

**平台 QID 速查（已核实）**：FC/NES `Q172742`、SFC/SNES `Q183259`、MD/Genesis `Q10676`、PS1 `Q10677`、PS2 `Q10680`、GBA `Q188642`、NDS `Q170323`、Switch `Q19610114`、Windows `Q1406`

### 10.5 Wikidata dump 与许可 —— ★ CC0

**【事实】** <https://dumps.wikimedia.org/wikidatawiki/entities/> 提供 JSON / N-Triples / Turtle（bz2 + gz），另有 truthy-NT 与 lexemes；最大文件约 **253 GB**（nt.gz）。

**【事实】许可**（<https://www.wikidata.org/wiki/Wikidata:Licensing> 原文）：
> "All structured data in the main, property and lexeme namespaces is made available under the **Creative Commons CC0 License** (Public domain)"

其他命名空间的文本为 CC BY-SA 4.0。

> **→ Wikidata 结构化数据 CC0：随便缓存、随便再分发、可商用、无需署名。本次调研的所有中文源里法律条件最好的。**

### 10.6 中文维基百科 API

**【事实】** Action API `https://zh.wikipedia.org/w/api.php`，REST `https://zh.wikipedia.org/api/rest_v1/page/summary/{title}` 实测可用：
```
title: 薩爾達傳說 時之笛
extract: 《薩爾達傳說 時之笛》是一款任天堂64的動作冒險電視遊戲作品…
thumbnail: https://upload.wikimedia.org/wikipedia/zh/5/5a/The_Legend_of_Zelda_Ocarina_of_Time_Boxart.jpg
```
> ⚠️ **默认返回繁体**（zh 主变体）。取简体需用 `?variant=zh-cn`，或 Action API 的 `&uselang=zh-cn` / `&variant=zh-hans`。

**【事实】内容许可**（实测 `action=query&meta=siteinfo&siprop=rightsinfo`）：
```json
{"url":"https://creativecommons.org/licenses/by-sa/4.0/deed.zh",
 "text":"Creative Commons Attribution-Share Alike 4.0"}
```

### 10.7 ★ 图片许可 —— 维基系不能作为封面图源

**【事实】Commons 明确拒绝合理使用**（<https://commons.wikimedia.org/wiki/Commons:Licensing> 原文）：
> "Wikimedia Commons does **not** accept content under the condition of fair use."
> "Republication and distribution **must** be allowed. Publication of derivative work **must** be allowed. **Commercial use** of the work **must** be allowed."
（且禁止 CC-NC 与 CC-ND）

**【事实】实测证实**：《薩爾達傳說 時之笛》封面 `File:The_Legend_of_Zelda_Ocarina_of_Time_Boxart.jpg`：
- 在 **Commons 上不存在**（`commons.wikimedia.org` API 返回 `"missing"`）
- 在 **zh.wikipedia 本地**存在，`extmetadata` 实测：`LicenseShortName = Fair use`、`NonFree = true`、`Copyrighted = True`、`UsageTerms = 合理使用…在「薩爾達傳說 時之笛」上下文中受版權保護的材料`

**结论【事实】**：
- Commons 上**基本没有游戏封面**
- zh.wikipedia **本地托管**封面，但明确标为**非自由/合理使用**，**不可缓存再分发**，只能在「该条目上下文」内使用
- Wikidata 的 `P18` 只能引用 Commons 上的自由图片 → 这正是 FC 游戏 P18 覆盖率只有 3.4% 的原因
- **维基系不能作为封面图源。**

### 10.8 中文源汇总对照

| 维度 | Bangumi | VNDB | Wikidata | zh.wikipedia | Steam appdetails |
|---|---|---|---|---|---|
| 注册 / 认证 | 只读免注册 | 只读免注册 | 免注册 | 免注册 | 免注册 |
| 收费 | 免费 | 免费（**仅非商业**） | 免费 | 免费 | 免费 |
| 明文速率 | **无**（文档无任何限流） | **200 req/5min + 1s CPU/min + 3s 超时** | 60s/query、5 并发/IP | 匿名无 UA **10/min**，带合规 UA **200/min** | **未文档化** |
| 强制 UA | 文档强制建议，实测未拦截 | 无 | **是**（不合规可能被完全封禁） | **是** | 无 |
| ROM 哈希 | **否** | **否**（但有 GTIN 条码 + 品番） | 否 | 否 | 否 |
| 封面图 | ✅ 5 档尺寸，CDN 无防盗链，ACAO:* | ✅ + rsync 全量 | ❌ 仅 3.4% | ⚠️ 有但**非自由** | ✅ header_image |
| 数据许可 | CC BY-SA | ODbL + DbCL | **CC0** ⭐ | CC BY-SA 4.0 | Steam 条款（灰色） |
| 图片可再分发 | ⚠️ 站方主张 Fair use | ⚠️ "fair use" | — | ❌ NonFree | ❌ |
| 离线 dump | ✅ **每周三，435 MB zip / 9 个 jsonlines，无图片** | ✅ 每日，db 188 MB + 图片 rsync ~42 GB | ✅ 每日，最大 253 GB | ✅ | ❌ |
| 游戏总量 | **87,188** | 65,933（全是 VN） | 175,739 | — | — |
| 中文覆盖 | **高**（`name_cn` 是一级字段） | 12% | 27–42% | 部分 | 简介 ≈100%，名部分 |
| 主机长尾 | 中（SFC 525 / N64 98 / DC 38） | ≈0 | 好（SFC 1,424 / FC 1,205） | 中 | ❌ 仅 PC |

> **【事实】最确定的一条负面结论：没有任何中文来源支持 ROM 哈希查询。** ROM → 中文条目的映射**必须自建** —— 先用 No-Intro/Redump DAT 做哈希识别得到规范名，再用名称+平台+年份匹配到 Bangumi/Wikidata。

---

## 11. 刮削器的多源合并与缓存实践

### 11.1 Skyscraper（muldjord）—— 资源缓存设计详解

一手来源：
- <https://github.com/muldjord/skyscraper/blob/master/docs/CACHE.md>
- <https://github.com/muldjord/skyscraper/blob/master/docs/CLIHELP.md>
- <https://github.com/muldjord/skyscraper/blob/master/docs/CONFIGINI.md>
- <https://github.com/muldjord/skyscraper/blob/master/docs/SCRAPINGMODULES.md>
- <https://github.com/muldjord/skyscraper/blob/master/src/cache.cpp>
- <https://github.com/muldjord/skyscraper/blob/master/src/nametools.cpp>
- <https://github.com/muldjord/skyscraper/blob/master/cache/priorities.xml.example>

#### 两阶段架构（最值得借鉴的设计）

**【事实】** Skyscraper 把工作严格拆成**两个独立阶段**：

1. **资源采集阶段**（`-s <MODULE>`）：从某个源抓数据，**只写进本地资源缓存**，不生成任何前端文件。
2. **游戏列表生成阶段**（**省略 `-s`**）：完全离线，从缓存里按优先级合并出 `gamelist.xml` + 合成图。

官方原文：
> "Skyscraper has two modes; resource gathering mode and gamelist generation mode. First you gather data into Skyscraper's resource cache by scraping the platform with any of the supported scraping modules (eg. `Skyscraper -p snes -s thegamesdb`). When you feel like you have gathered all the resources that you need, you then generate the gamelist by simply leaving out the `-s MODULE` option."

来源：<https://github.com/muldjord/skyscraper/blob/master/docs/FAQ.md>

设计意图写得很直白：
> "Think of the resource cache as the cache in an internet browser… It helps keep the online servers healthy by not hammering them whenever you need resources you already downloaded once. And it allows you to re-generate the frontend game lists if you add new games or perhaps want to change the style of the exported artwork."

来源：<https://github.com/muldjord/skyscraper/blob/master/docs/CACHE.md>

#### 缓存目录结构

**【事实】** 默认根目录 `/home/USER/.skyscraper/cache/`，下面每个平台一个**自包含**子目录：

```
~/.skyscraper/cache/
├── priorities.xml.example        # 模板，勿手改（升级会覆盖）
└── <PLATFORM>/                   # 例如 snes/，可整目录拷到别的机器
    ├── db.xml                    # 资源数据库（文本资源 + 媒体文件的相对路径）
    ├── quickid.xml               # 文件路径 -> cacheId 的快速映射（见下）
    ├── priorities.xml            # 本平台的各字段源优先级
    ├── covers/<MODULE>/<ID>      # 例如 covers/screenscraper/<sha1hex>
    ├── screenshots/<MODULE>/<ID>
    ├── wheels/<MODULE>/<ID>
    ├── marquees/<MODULE>/<ID>
    └── videos/<MODULE>/<ID>
```

目录创建逻辑在 `Cache::createFolders(scraper)`，对 `covers/screenshots/wheels/marquees/videos` 各建一个 `<scraper>` 子目录；同时若 `priorities.xml` 不存在就从模板拷贝一份。
来源：<https://github.com/muldjord/skyscraper/blob/master/src/cache.cpp>（第 47–74 行）

媒体的相对路径由 `Cache::addResources` 拼成 `"<type复数>/" + entry.source + "/" + entry.cacheId`，写入 `db.xml` 的 `resource` 值。
来源：同上（第 1440–1495 行）

**【事实】** 缓存里的图片默认会被**缩小并统一转成无损 PNG**：
> "By default, to save space, Skyscraper resizes large pieces of artwork before adding them to the resource cache. Setting this option to `"false"` will disable this… Beware that Skyscraper converts all artwork resources to lossless PNG's when saving them."

（`cacheResize` 选项）来源：<https://github.com/muldjord/skyscraper/blob/master/docs/CONFIGINI.md>
另有 `cacheCovers` / `cacheScreenshots` / `cacheWheels` / `cacheMarquees` 逐类型开关，以及 `videoConvertCommand`（下载后调外部命令转码，`%i`/`%o` 占位）。

#### db.xml 的记录格式与去重键

**【事实】** 单条记录：
```xml
<resource id="<ID KEY>" type="<RESOURCE TYPE>" source="<SCRAPING SOURCE>"
          timestamp="<UNIX TIMESTAMP IN MSECS>">Resource data</resource>
```
> "NOTE! Pre-3.3.0 versions of Skyscraper used `sha1` as the name of the unique id key. Later versions use `id`."

资源类型共 15 种：`title`、`platform`、`description`、`publisher`、`developer`、`players`、`tags`、`releasedate`、`ages`（最小允许年龄）、`rating`（0–1 的小数）、`cover`、`screenshot`、`wheel`、`marquee`、`video`。

**去重键是三元组 `(cacheId, type, source)`** —— 即「每个游戏 × 每种资源 × 每个源」各存一份：
```cpp
if(res.cacheId == resource.cacheId && res.type == resource.type && res.source == resource.source) {
  if(config.refresh) { it.remove(); } else { notFound = false; }
}
```
来源：<https://github.com/muldjord/skyscraper/blob/master/src/cache.cpp>（`Cache::addResource`，第 1463–1490 行）

> **这是多源合并的基础**：不是「后来的覆盖先来的」，而是**并存所有源的版本**，到生成阶段再按优先级挑。

#### cacheId 的计算 —— 「ROM 内容哈希」而非文件名

**【事实】** `NameTools::getCacheId()`：默认对**文件内容**算 SHA1（每次读 1024 字节流式喂入），但有三个例外会退化为**对文件名算 SHA1**：

```cpp
QCryptographicHash cacheId(QCryptographicHash::Sha1);
bool cacheIdFromData = true;
// 1) 脚本类或"不稳定"的压缩/索引文件
if(suffix == "uae" || "cue" || "sh" || "svm" || "scummvm" || "mds"
   || "zip" || "7z" || "gdi" || "ml" || "bat" || "au3") cacheIdFromData = false;
// 2) 大于 50 MB 的文件（性能优化）
if(info.size() > 52428800) cacheIdFromData = false;
// 3) 空文件
if(info.size() == 0) cacheIdFromData = false;
```
来源：<https://github.com/muldjord/skyscraper/blob/master/src/nametools.cpp>（第 494–532 行）

> **注意 `zip`/`7z` 在退化名单里** —— 因为压缩包字节流不稳定（时间戳/压缩级别都会改变哈希）。这与 §1.4 里为 ScreenScraper **解压后再算哈希**是两套不同的哈希：一套是**本地缓存主键**（cacheId），一套是**远端查询键**（crc/md5/sha1）。自建工具需要同样区分这两个概念。

#### quickid.xml —— 避免重复读盘算哈希

**【事实】** 对大 ROM 反复算 SHA1 很贵，Skyscraper 用 `quickid.xml` 做一层 memo：

```xml
<quickids>
  <quickid filepath="<绝对路径>" timestamp="<文件 mtime 毫秒>" id="<cacheId>"/>
</quickids>
```

读取逻辑：
```cpp
QString Cache::getQuickId(const QFileInfo &info) {
  if(quickIds.contains(info.absoluteFilePath()) &&
     info.lastModified().toMSecsSinceEpoch() <= quickIds[info.absoluteFilePath()].first) {
    return quickIds[info.absoluteFilePath()].second;   // 命中：直接返回，不读文件
  }
  return QString();                                     // 未命中：调 getCacheId 重算
}
```
写入时记录的是**文件的 mtime**，所以 ROM 一旦被改动（mtime 变新）缓存立即失效并重算。
来源：<https://github.com/muldjord/skyscraper/blob/master/src/cache.cpp>（第 78–100、1167–1190、1687–1701 行）

调用点固定成这个模式：
```cpp
QString cacheId = getQuickId(info);
if(cacheId.isEmpty()) { cacheId = NameTools::getCacheId(info); addQuickId(info, cacheId); }
```

**【事实】** `Cache::write(bool onlyQuickId)` 支持**只写 quickid 不写 db.xml**，用于纯查询流程结束时持久化新算出的映射。

#### priorities.xml —— 字段级源优先级

**【事实】** 每平台一个 `priorities.xml`，按**资源类型**分别定义源顺序：

> "if you know that `thegamesdb` always provides the best `descriptions` for games, you'd add an `<order type="description">` node with a `<source>thegamesdb</source>` subnode. You can have multiple `<source>` nodes, Skyscraper will then prefer the topmost source when generating a game list. **If the topmost isn't found it'll prioritize the next one and so forth. Any source that isn't listed with an `<order>` node will be prioritized using timestamps** for when each resource was added to the cache."

来源：<https://github.com/muldjord/skyscraper/blob/master/docs/CACHE.md>

官方模板（节选，展示不同字段用不同顺序）：
```xml
<order type="title">
  <source>import</source><source>esgamelist</source><source>openretro</source>
  <source>arcadedb</source><source>screenscraper</source>
  <source>mobygames</source><source>thegamesdb</source>
</order>
<order type="platform">
  <source>screenscraper</source><source>thegamesdb</source><source>mobygames</source>
</order>
<order type="releasedate">
  <source>import</source><source>esgamelist</source><source>arcadedb</source>
  <source>screenscraper</source><source>openretro</source>
  <source>thegamesdb</source><source>mobygames</source>
</order>
<order type="developer">
  <source>import</source><source>esgamelist</source><source>arcadedb</source><source>mobygames</source>
</order>
```
来源：<https://github.com/muldjord/skyscraper/blob/master/cache/priorities.xml.example>

要点：
- **本地/人工来源永远排最前**（`import` > `esgamelist` > 各在线源）—— 用户手工修正永不被在线数据覆盖。
- **不同字段的最佳源不同**：`title` 首选 openretro/arcadedb，`platform` 首选 screenscraper，`developer` 首选 mobygames。
- **未列出的源按时间戳排序**（新的优先），所以模板不必穷举。
- `--cache edit` 手工新增的资源"will be prioritized above all others"（优先于一切）。

#### 区域 / 语言的字段级 fallback

**【事实】区域（影响标题与全部媒体）**：Skyscraper 会**从文件名自动识别区域**（识别 `(Europe)`、`(e)` 等），把识别到的区域顶到优先级表首位；找不到该区域的资源就沿列表往下走：

> "If info or media isn't found for the auto-detected region, it will move down the list and check the next region on the list until it finds one that has data for the requested resource."

默认区域优先级链：`--region 指定值` → 文件名自动识别值 → `eu, us, ss, uk, wor, jp, au, ame, de, cus, cn, kr, asi, br, sp, fr, gr, it, no, dk, nz, nl, pl, ru, se, tw, ca`
来源：<https://github.com/muldjord/skyscraper/blob/master/docs/REGIONS.md>

**【事实】语言（只影响简介与 genres/tags）**：默认链为 `--lang 指定值 / 自动识别` → `en` → `de` → `fr` → `es`。文档明确区分二者：
> "It is important to understand the distinction between game region and language. Setting a language is a user-preferred thing and will only affect the game descriptions and tags (genres). The remaining game data is tied to the region instead (artwork and, in some cases, the game name)."

来源：<https://github.com/muldjord/skyscraper/blob/master/docs/LANGUAGES.md>

> **【推断】对中文项目的直接启示**：把 `zh` 顶到语言链首、`cn`/`tw` 顶到区域链靠前位置，就能在 ScreenScraper 上优先取中文简介和中文区封面；但由于覆盖率问题，必须保留 `en`/`ja` 作为 fallback，并从别的源（见 §9）补中文标题。

#### 缓存运维命令（自建工具应对标的能力清单）

**【事实】** 来源：<https://github.com/muldjord/skyscraper/blob/master/docs/CLIHELP.md>

| 命令 | 作用 |
|---|---|
| `--cache show` | 显示各模块各类型的缓存条目统计 |
| `--cache edit[:new=<TYPE>]` | 交互式增删单个 ROM 的资源；手工添加的优先级最高；支持批量插入同类型 |
| `--cache purge:all` / `:m=<MODULE>` / `:t=<TYPE>` / `:m=X,t=Y` | 按模块/类型定向清除（不可撤销） |
| `--cache vacuum` | 清除**与当前 ROM 集合无关联**的所有资源（删了 ROM 后回收空间） |
| `--cache validate` | 校验完整性：清掉 db.xml 里有记录但文件丢失、或文件在但无记录的孤儿 |
| `--cache merge:<FOLDER>` | **把另一个缓存合并进当前缓存**（跨机器共享成果） |
| `--cache refresh` / `--refresh` | 强制用新数据覆盖已缓存资源 |
| `--cache report:missing=<all\|textual\|artwork\|media\|type1,type2>` | 导出缺失某资源的文件名清单到 `~/.skyscraper/reports/report-<PLATFORM>-missing_<RESOURCE>-yyyymmdd.txt` |
| `--fromfile <FILE>` | 只处理清单文件里的 ROM |

**`report` + `fromfile` 的组合是配额友好的核心工作流**：先用 A 源全量刮，导出「缺 developer 的清单」，再只对这批文件用 B 源刮 —— 把稀缺配额精确花在缺口上。官方明确提示了这个用法：
> "NOTE! The reports can be fed back into Skyscraper using the `--fromfile` option… This is useful in combination with, for instance, the `--cache edit` option or the `--cache refresh`/`--refresh` option(s)."

**【事实】** 缓存目录是**可移植**的：
> "Each subfolder in the `/home/USER/.skyscraper/cache/` folder is self-contained and can be copied to other Skyscraper installations at your convenience."

#### Skyscraper 各模块的能力与限制对照（官方文档原文数字）

来源：<https://github.com/muldjord/skyscraper/blob/master/docs/SCRAPINGMODULES.md>

| 模块 | 类型 | 匹配方式 | 凭据 | 请求限制 | 线程 | 媒体 |
|---|---|---|---|---|---|---|
| `screenscraper` | 在线 | **ROM 校验和** + 精确文件名 | 强烈建议 | 注册用户 20k/天 | 1+，看等级 | cover, screenshot, wheel, marquee, video |
| `thegamesdb` | 在线 | 文件名搜索 | 不需要 | **3000 请求 / IP / 月** | 无 | cover, screenshot, wheel, marquee |
| `arcadedb` | 在线 | **MAME 文件名 ID** | 不需要 | 无 | 1 | cover, screenshot, wheel, marquee, video |
| `openretro` | 在线 | WHDLoad uuid + 文件名 | 不需要 | 无 | 1 | cover, screenshot, marquee |
| `mobygames` | 在线 | 文件名搜索 | 不需要 | **1 请求 / 10 秒**，每次运行最多 35 个 ROM | 1 | cover, screenshot |
| `igdb` | 在线 | 文件名搜索 | **必需**（Twitch client-id + secret） | 最多 4 请求/秒 | 4（各 1 req/s） | **无** |
| `worldofspectrum` | 在线 | 文件名搜索 | 不需要 | 无 | 无 | cover, screenshot |
| `esgamelist` | **本地** | 精确文件名 | — | 无 | 无 | screenshot, marquee, video |
| `import` | **本地** | 精确文件名 | — | 无 | 无 | cover, screenshot, wheel, marquee, video |

官方对 MobyGames 的警告值得注意：
> "This restriction is **global for the entire Skyscraper user base**, which means that it might quit on you if other users are currently scraping from it. For this reason it has been strongly limited inside of Skyscraper by forcing a maximum number of rom scrapings per run. Please use this module sparingly."

官方推荐的多源顺序：
> "I would recommend scraping your roms with `screenscraper` first, and then use `thegamesdb` to fill out the gaps in your cache."

关于 ScreenScraper 精确文件名匹配在街机上的坑：
> "*Exact* file name matching does not work well for the `arcade` derived platforms in cases where a data checksum doesn't match. The reason being that `arcade` and other arcade-like platforms are made up of several subplatforms. Each of those subplatforms have a high chance of containing the same file name entry. In those cases ScreenScraper can't determine a unique game and will return an empty result."

**【事实】IGDB 只提供文本、不提供媒体**，作者给出的理由是 IGDB 不按平台区分美术资源：
> "the database doesn't distinguish between platform versions of the same game when it comes to any artwork resources… This makes it less usable in a retro game scraping context as many of the games differ drastically visually between the old platforms. For that reason alone, this module will only provide textual data for your roms for the time being."

### 11.2 ES-DE（EmulationStation Desktop Edition）

一手来源（仓库 <https://gitlab.com/es-de/emulationstation-de>，许可 MIT）：
- <https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md>
- <https://gitlab.com/es-de/emulationstation-de/-/raw/master/es-app/src/scrapers/ScreenScraper.cpp>
- <https://gitlab.com/es-de/emulationstation-de/-/raw/master/es-app/src/scrapers/ScreenScraper.h>
- <https://gitlab.com/es-de/emulationstation-de/-/raw/master/es-app/src/scrapers/GamesDBJSONScraper.cpp>
- <https://gitlab.com/es-de/emulationstation-de/-/raw/master/es-core/src/Settings.cpp>
- <https://gitlab.com/es-de/emulationstation-de/-/raw/master/es-core/src/HttpReq.cpp>

**【事实】只支持两个源**：`es-app/src/scrapers/` 下只有 `ScreenScraper.*` 与 `GamesDBJSONScraper*`，没有 ArcadeDB/IGDB 等。默认源是 ScreenScraper（`Settings.cpp`：`mStringMap["Scraper"] = {"screenscraper", "screenscraper"};`）。

USERGUIDE 官方能力对照表：

| 媒体/选项 | ScreenScraper | TheGamesDB |
|---|---|---|
| Region (EU/JP/US/WOR) | Yes | **No** |
| Multi-language | Yes | **No** |
| Game names / Other metadata | Yes | Yes |
| Ratings | Yes | **No** |
| Videos | Yes | **No** |
| Screenshots / Title screens / Box covers / Box back covers / Marquees(wheels) / Fan art | Yes | Yes |
| 3D boxes / Physical media / Game manuals (PDF) | Yes | **No** |

**【事实】哈希：只发 md5，不发 crc / sha1。** 对 `es-app/src/scrapers/` 全目录 grep `crc|sha1` 无结果。发送形式：
```cpp
if (md5Hash != "") {
    screenScraperURL.append("&md5=").append(md5Hash)
        .append("&romtaille=").append(std::to_string(fileSize));
}
```

**【事实】文件大小上限：默认 384 MiB，UI 可调 32–800 MiB**：
```cpp
mIntMap["ScraperSearchFileHashMaxSize"] = {384, 384};
mBoolMap["ScraperSearchFileHash"] = {true, true};
// GuiScraperMenu.cpp 钳制：< 32 → 32；> 800 → 800
```
触发条件（`GuiScraperSearch.cpp`）：
```cpp
// Only use MD5 file hash searching when in automatic mode.
if (mSearchType == AUTOMATIC_MODE &&
    Settings::getInstance()->getBool("ScraperSearchFileHash") &&
    Settings::getInstance()->getString("Scraper") == "screenscraper" && params.fileSize != 0 &&
    params.fileSize <= Settings::getInstance()->getInt("ScraperSearchFileHashMaxSize") * 1024 * 1024) {
```
要点：**只在自动模式算哈希**（USERGUIDE："File hash searching is not supported by ScreenScraper if using this search method"）；哈希在独立线程算以免卡 UI；超限只写 debug 日志并回退到名称搜索。

**【事实】端点选择**：`if (automaticMode || singleSearch)` → `jeuInfos.php`，否则 → `jeuRecherche.php`。`singleSearch` 在「街机系统且未启用按元数据名搜索」「搜索名短于 4 字符」等情况为 true。
**`ssuserInfos.php` 完全未被使用**（全仓库 grep 为空）。

**【事实】devid/devpassword：编译进二进制，XOR 混淆**：
```cpp
const std::string API_DEV_U {15, 21, 39, 22, 42, 40};
const std::string API_DEV_P {32, 70, 46, 54, 12, 5, 13, 120, 50, 66, 25};
const std::string API_DEV_KEY {67, 112, 72, 120, ...};
// StringUtil.cpp: buffer[i] = input[i] ^ key[i];
```
**TheGamesDB 的 API key 连混淆都没有，明文常量**：
```cpp
constexpr char GamesDBAPIKey[] {"e42de8fb0cdcea89fec60f70cd565122f34f5c6228be3e9ae247ba409779d9d5"};
```
（`GamesDBJSONScraperResources.cpp`）—— **无代理、无中继**，直接 `?apikey=` 拼接。

**【事实】用户账号**：`ScraperUseAccountScreenScraper` 默认 true 但用户名密码默认空 → 不填即匿名（走 SS guest 线程池）。USERGUIDE 警告："Be aware that the es_settings.xml file contains the password in clear text."。认证失败靠响应里 `<ssuser><niveau>` == "0" 判定。

**【事实】线程限制：ES-DE 完全不做。** grep `maxthreads`/`maxrequestspermin` 无结果；只读 `requeststoday`/`maxrequestsperday` 用于 UI 显示剩余额度。刮削是**串行队列**（`std::queue<std::unique_ptr<ScraperRequest>>`，每次只处理队首一个）。
> **【推断】** 好处是绝不会撞 429；代价是付费账号的多线程加速用不上。

**【事实】Region / Language 回退链**（默认 `ScraperRegion="eu"`、`ScraperLanguage="en"`）：
- 游戏名 / 发行日期：`{region, "wor", "us", "ss", "eu", "jp"}`
- 简介 synopsis：按 `langue` 属性 `{language, "en", "wor"}`
- 类型 genre：`{language, "en"}`
- **媒体**（与文本链不同）：`{region, "wor", "us", "eu", "jp", "cus", otherRegion}`，`otherRegion` 仅在 `ScraperRegionFallback`（默认 true）开启时取返回结果里的第一个 region
- `video`/`video-normalized`/`fanart` 无 region 属性，直接取第一条

**【事实】媒体类型字符串**（`ScreenScraper.h`）：
```cpp
media_3dbox = "box-3D";          media_backcover = "box-2D-back";
media_cover = "box-2D";          media_fanart = "fanart";
media_marquee = "wheel";         media_marquee_hd = "wheel-hd";
media_physicalmedia = "support-2D";
media_screenshot = "ss";         media_titlescreen = "sstitle";
media_video = "video";           media_video_normalized = "video-normalized";
media_manual = "manuel";
```
**机制要点：ES-DE 不在请求 URL 里指定 media 参数** —— 发一次 `jeuInfos.php` 拿完整 XML，再在客户端按 XPath `media[@type='<type>']` 过滤。

**【事实】错误处理**（`HttpReq.cpp`）：
```cpp
if (responseCode == 429 && Settings::...getString("Scraper") != "screenscraper") { REQ_QUOTA_REACHED; }
else if (responseCode == 430 && Settings::...getString("Scraper") == "screenscraper") { "You have exceeded your daily scrape quota"; }
else if (responseCode == 404 && ...getBool("ScraperIgnoreHTTP404Errors")) { REQ_RESOURCE_NOT_FOUND; }
```
即 **SS 配额用 430 判定，429 只对 TGDB 生效**；对 SS 的 429「maximum threads」无专门处理（因串行不会撞）。另有 `ZZZ(notgame)` 过滤、重试 3 次间隔 3 秒（`ScraperRetryOnErrorCount=3`、`ScraperRetryOnErrorTimer=3`）。

**【事实】媒体存放路径**：`~/ES-DE/downloaded_media/<system name>/<media type>/`，媒体子目录固定 13 个：
```
3dboxes / backcovers / covers / custom / fanart / manuals / marquees /
miximages / physicalmedia / screenshots / titlescreens / videos
```
**miximage 由 ES-DE 本地合成**（`es-app/src/MiximageGenerator.cpp`："Generates miximages from screenshots, marquees, 3D boxes/covers and physical media images."），默认 1280x960 PNG。
> **这与 Batocera 形成鲜明对比** —— Batocera 用 SS 服务端的 mix，一张合成图消耗 3 次配额；ES-DE 本地合成不额外消耗配额。

**【事实】本地缓存：几乎没有。** 除媒体文件与 gamelist.xml 外，只缓存 TheGamesDB 的三张静态查表：
```
<appdata>/scrapers/thegamesdb_developers.json
<appdata>/scrapers/thegamesdb_publishers.json
<appdata>/scrapers/thegamesdb_genres.json
```
其余只有一个内存态缩略图缓存（`Scraper.h`：`std::string thumbnailImageData; // Thumbnail cache`）。**没有任何持久化的搜索结果缓存 / 本地游戏数据库 —— 每次刮削都重新打 API。**
> 这是 ES-DE 与 Skyscraper / Skraper 的**根本性架构差异**。

### 11.3 Skraper

一手来源：<https://www.skraper.net/>（正文由 i18n JSON 驱动，权威原文在 <https://www.skraper.net/locales/en/skraper.json>）

**【事实】闭源免费软件，非开源。** 官网 About："**SKRAPER is a FREE, non-commercial, non-profit application** made by retrogaming fans for retrogaming fans."。全站无源码仓库链接，唯一分发物是二进制包 <https://www.skraper.net/download/Skraper-1.4.2.7z>。.NET 应用（Windows 需 .NET Framework 4.8.1；Linux/macOS 需 mono-complete 或 WINE，官方文本在这点上自相矛盾）。作者署名 "Skraper: **Bkg2k & Maksthorr**"。

**【事实】数据源 = ScreenScraper**：官网 "All metadata provided by **ScreenScraper.fr**"、"It relies on the **ScreenScraper Online Database** feed by the community."。ScreenScraper.fr 自己的全站导航栏里就有 "SKRAPER" 按钮。
**【未确认】** 但**没有任何官方文字声明 Skraper 由 ScreenScraper 团队开发/拥有**，正式隶属关系无一手证据（只有大量间接证据）。

**【事实】媒体与输出**：
- 图片："Game's Logo, Screenshots, Flyers, 3D-boxes, SteamGrid..."；文本："Synopsis, Genres, Classifications, Number of players, Ratings..."；"Information, Pictures, Videos or even **Manuals**"
- 1.4 新增："Game maps scraping / User manuals scraping / **Pad-2-Keyboard** scraping / Tips & Tricks scraping"
- **自定义合成图引擎**："**XML Mix descriptors** allow creating complex image compositions, using remote & local resources. Position, resize, rotate, apply color filters & project images, text or even sub-mixes..." → 比 ES-DE 的固定版式 miximage 灵活得多
- 支持的前端（官网原文）："Skraper currently supports **EmulationStation** metadata through **RecalBox & RetroPie**."、"It can fill **LaunchBox** game list & images"。计划中："MAME/MAMEUI"
- **【未确认】** Attract Mode / Pegasus / Batocera 支持 —— 官网完全未提及

**【事实】数据回传条款**（官网明示）：
> "By using SKRAPER, you allow implicitly the ScreenScraper.fr database to **anonymously and automatically record the names and checksums of your roms/games**. Collected data will be verified and may be integrated into the database."

**【事实】本地缓存是其核心卖点**：
> "**Local Cache** — Test as many configurations as you want. With the local cache, **download once, scrap many**"
> "**Local cache** + Massive optimization & multithreading usage = Less bandwidth, more speed!"
> "Optimized media storage. **Hash computation allow media to be saved once, linked many**"（即用哈希做媒体去重，同一张图多游戏共享）
> 1.4："Optimized already downloaded media & **cache management**"、"**Cleaning unused media files when scraping again**"

**【未确认】** 缓存的具体落盘格式与位置（闭源，官网无 docs/FAQ 页，`/faq`、`/docs`、`/robots.txt` 全 404）。

**【事实】许可**：无 LICENSE / EULA，只有免责声明（"provided \"as is\", without warranty of any kind…"）+ "This site is not affiliated to any kind of hardware or videogame company."。免费，站内无付费入口（付费联动在 SS 侧）。
**【事实】** 当前版本 1.4.2（<https://www.skraper.net/update/update.json>：`{"Version":"1.4.9726.39185","UserVersion":"1.4.2","Mandatory":true,...}`，`Mandatory: true` 强制更新）；服务器 `Last-Modified: Tue, 18 Aug 2026`。仍在积极维护（官方："Skraper's development is **now actively resumed by Maksthorr**"）。

### 11.4 Batocera / Recalbox / RetroPie / ArcadeDB

#### Batocera —— 源最多，且 dev 凭据不在公开仓库

**【事实】**（<https://raw.githubusercontent.com/batocera-linux/batocera-emulationstation/master/es-app/src/scrapers/Scraper.cpp>）：
```cpp
#define OVERQUOTA_RETRY_DELAY 15000
#define OVERQUOTA_RETRY_COUNT 5
std::vector<std::pair<std::string, Scraper*>> Scraper::scrapers {
#ifdef SCREENSCRAPER_DEV_LOGIN
	{ "ScreenScraper", new ScreenScraperScraper() },
#endif
#ifdef GAMESDB_APIKEY
	{ "TheGamesDB", new TheGamesDBScraper() },
#endif
#ifdef HFS_DEV_LOGIN
	{ "HfsDB", new HfsDBScraper() },
#endif
	{ "IGDB", new IGDBScraper() },
	{ "ArcadeDB", new ArcadeDBScraper() }
};
```
> **关键**：SS / TGDB / HfsDB 的开发者凭据是**编译期宏，公开仓库里没有** —— 只有官方构建才启用。IGDB 用用户自己的 Client ID/Secret，ArcadeDB 无需 key。这是比 ES-DE「XOR 混淆常量放仓库里」更规矩的做法。

**【事实】** Batocera 的 SS 实现**同时发 md5 + crc**，大小上限 **128 MiB**，且能从压缩包内提取哈希：
```cpp
if (length > 1024 * 1024 && !md5.empty()) {           // >1MB 用已有元数据里的哈希
    path += "&md5=" + toUpper(md5);
    if (!crc32.empty()) path += "&crc=" + toUpper(crc32);
} else {
    if (length > 0 && length <= 131072 * 1024)         // 128 MiB max
        md5 = ApiSystem::getMD5(fileNameToHash, system->shouldExtractHashesFromArchives());
}
path += "&romtaille=" + std::to_string(length);
```
**【事实】** 显式处理线程超限：
```cpp
if (toLower(response).find("maximum threads per minute reached") != std::string::npos) return false;
```
**【事实】** 用 `ssuserInfos.php` 读 `maxthreads` 决定并发数：
```cpp
int ScreenScraperScraper::getThreadCount(...) { if (userInfo.maxthreads > 0) return userInfo.maxthreads; return 1; }
```
读取的 ssuser 字段全集：`id / maxthreads / requeststoday / requestskotoday / maxrequestspermin / maxrequestsperday / maxrequestskoperday`。

**【事实】** region 回退链把 language 放最前：
```cpp
for (auto _region : std::vector<std::string>{ language, region, "wor", "us", "eu", "jp", "ss", "cus", "" })
```
每种图另有 rip list 回退链：
```cpp
"wheel"   → { wheel, wheel-hd, wheel-steel, wheel-carbon, screenmarqueesmall, screenmarquee }
"marquee" → { screenmarqueesmall, screenmarquee, wheel, wheel-hd, wheel-steel, wheel-carbon }
"box-2D"  → { box-2D, box-3D }
"ss"      → { ss, sstitle }
```
**【事实】** 媒体落盘：`<ROM 目录>/<system>/images/<游戏名>-<后缀>.<ext>`，后缀 `image/thumb/marquee/video/fanart/boxback/box/wheel/titleshot/manual/magazine/map/cartridge/bezel`。

**【事实】官方 wiki 的配额说明**（<https://wiki.batocera.org/scrape_from>）：
> "there is also a limitation in the number of assets you can download each day: **50,000 requests max per day**. Also, if you use a \"mix\" for the image, be aware that you need to download a screenshot + box + game title/marquee, which are **3 downloads for 1 composite image**."
> "In order to use ScreenScraper, you need to **create a login on their website**"
> "IGDB has been introduced with **Batocera 42**, and it requires to enter IGDB Client ID and Client secret... you need a **Twitch account**"

#### Recalbox —— 内置只暴露 ScreenScraper

**【事实】**（<https://gitlab.com/recalbox/recalbox-emulationstation/-/raw/master/es-app/src/scraping/ScraperFactory.cpp>）：
```cpp
static std::vector<std::string> _List = {
    "Screenscraper",
    //"TheGamesDB",       // 引擎存在但在 UI 里被注释掉
};
```
（官方 wiki 的对照表写 "ScreenScraper, TheGamesDB, MameDB-mirror"，**与源码不一致，源码为准**。）

**【事实】** Recalbox 的 SS 调用用 **JSON**（ES-DE 用 XML），**同时发 crc + md5**，无 `jeuRecherche.php`（无交互式搜索）：
```cpp
result.append("output=json");
result.append("&devid=").append(XOrTheSpaceSheriff(API_DEV_U, API_DEV_K));   // 同样是 XOR 混淆
...&romtype=rom&systemeid=<id>&romnom=<filename>&romtaille=<size>[&crc=][&md5=]
```
**【事实】** 线程数直接取自 SS 账号：`int threadCount = apis.GetUserInformation().mThreads; mRunner.Run(threadCount, true);`
**【事实】** HTTP 状态码映射（可直接抄的实现范例）：
```cpp
case   0:
case 429: Thread::Sleep(5000); continue;              // 线程超限 → 睡 5 秒重试
case 426: case 423: case 403: case 401: case 400: FatalError; break;
case 404: NotFound; break;
case 431: case 430: QuotaReached; break;
case 200: Ok; break;
```

**【事实】官方 wiki 的配额算法说明**（<https://wiki.recalbox.com/en/basic-usage/features/internal-scraper>，**极有价值**）：
> "A scrap of a game = **1 quota request for all textual information AND 1 quota request PER IMAGE/VIDEO existing**! If you have a game with 1 image, 1 thumbnail, 1 p2k, 1 video, 1 map and 1 manual, and everything is available, you'll use **7 requests** from your quota. From a quota of **15,000 per day**, for this game, you'll be down to a quota of 14,993."

> **注意**：Recalbox wiki 说 15,000/天、Batocera wiki 说 50,000/天、Skyscraper 文档说 20k/天 —— 三者都是各自的官方/实现方文档，差异**【推断】**源于账号等级不同或文档时点不同。**自建工具不应硬编码任何数字，必须读 `maxrequestsperday`。**

#### RetroPie 内置 EmulationStation —— 永远匿名

**【事实】**（<https://raw.githubusercontent.com/RetroPie/EmulationStation/master/es-app/src/scrapers/Scraper.cpp>）支持 TheGamesDB（默认）与 ScreenScraper。
**关键发现**：`getGameSearchUrl()` 只拼 `devid/devpassword/softname/output/romnom`，**没有 `ssid`/`sspassword`** → 永远走 SS 的 guest 线程池（上限 256）。这正是 Skraper/Skyscraper/sselph 等外部工具在实用性上胜出的结构性原因。默认只抓一种图 `media_name = "box-2D"`。仓库未归档，最后提交 2026-07-03，但最后 release 停在 v2.11.2 / 2023-04-13。

**【事实】RetroPie 官方文档**（<https://raw.githubusercontent.com/RetroPie/RetroPie-Docs/master/docs/Scraper.md>）列出三种：
> "* The built-in EmulationStation scraper, which pulls information from **TheGamesDb.net or ScreenScraper.fr**.
> * Steven Selph's Scraper (`scraper`), which pulls information from **TheGamesDB.net, ScreenScraper.fr, OpenVGDB, ArcadeItalia.net**.
> * Skyscraper (`skyscraper`), which pulls information from **TheGamesDB.net, ScreenScraper.fr, OpenRetro.org, MobyGames.com, IGDB.com and WorldOfSpectrum.org**."

**【事实】Universal XML Scraper 已停更，作者官方指向 Skraper**（<https://raw.githubusercontent.com/Universal-Rom-Tools/Universal-XML-Scraper/master/README.md>）：
> "After a lot's of health problem I cannot continue to work on UXS... I think you can easily **migrate to Skraper ( https://www.skraper.net/ )** better support and much more nice code ^^ ... Sincerly, Screech."

最后代码提交 2018-02-12，**无许可证**（法律上"保留所有权利"）。数据源仅 ScreenScraper。

#### ArcadeDB (adb.arcadeitalia.net) —— 街机/MAME 媒体的免 key 源

一手来源：<http://adb.arcadeitalia.net/service_scraper.php>

**【事实】有公开 API，无需 key、无需账号。** Base `https://adb.arcadeitalia.net/service_scraper.php`：

| 函数 | URL | 参数 |
|---|---|---|
| QUERY_MAME | `?ajax=query_mame` | `game_name`（必填，如 `mslug`）、`lang`(`it\|en`) |
| QUERY_MAME_LIKE | `?ajax=query_mame_like` | `game_name` |
| QUERY_MAME_MEDIA | `?ajax=query_mame_media` | `game_name` |
| DOWNLOAD_STATUS | `?ajax=download_status` | 无 |

（Batocera 实际调用：`?ajax=query_mame&lang=en&use_parent=1&game_name=<name>`）

**【事实】** 响应 JSON/UTF-8，顶层 `release` + `result`；维护时返回 503。
QUERY_MAME_MEDIA 返回的完整媒体集（png）：`url_icon / url_image_artwork_preview / url_image_bezel / url_image_boss / url_image_cabinet / url_image_cpanel / url_image_flyer / url_image_ingame / url_image_howto / url_image_logo / url_image_marquee / url_image_pcb / url_image_box / url_image_score / url_image_select / url_image_decal / url_image_title / url_image_versus / url_image_gameover / url_image_end / url_image_warning`，另有 `url_manual`(PDF)、`youtube_video_id`、`url_video_shortplay`、`url_video_shortplay_hd`。

**【事实】限制条款（原文）**：
> "it is **strongly recommended to use a single connection per ip address at a time**, that is a single thread to query and download files for each user. In case of bandwidth problems, I could set limits on these connections or not allow downloading files."
> "Whenever possible **join together the calls**. For example, looking for 10 romsets information instead of making 10 distinct calls."
> "If you are using [GET], pay attention to the number of characters in the url and keep it to a **maximum of 800**."

**【事实】署名要求（原文）**：
> "The returned information should always be used by **citing the source, authors, and copyrights**. You can use one or more of the following data to indicate the source: **Scraping using Arcade Database by motoschifo** / https://adb.arcadeitalia.net / arcadedatabase@gmail.com"
> 并要求先打招呼："before you start using this page, let me know it with a message so that I can add your name to the thanks/credits page"

**【事实】版权立场（原文）**：
> "All names and images are used here for informational purposes only (**\"Fair Use\" usage, per 17 U.S.C. Section 107**)... All copyrights and trademarks belong to their respective copyright and trademark holders."

### 11.5 各前端刮削实现横向对比

| | Skyscraper | ES-DE | Batocera ES | Recalbox ES | Skraper | RetroPie ES |
|---|---|---|---|---|---|---|
| 刮削源 | SS/TGDB/ArcadeDB/OpenRetro/MobyGames/IGDB/WoS/本地×2 | SS + TGDB | SS+TGDB+ArcadeDB+IGDB(+HfsDB) | 仅 SS | 仅 SS | SS + TGDB |
| 发送哈希 | **crc + md5 + sha1** | 仅 md5 | md5 + crc | md5 + crc | 有本地哈希缓存 | **无** |
| 哈希大小上限 | 压缩包 ~78 MB 才解压 | **384 MiB**（32–800 可调） | **128 MiB** | 【未确认】 | 【未确认】 | N/A |
| 读 `ssuserInfos.php` | 否（从 jeuInfos 响应取） | **否** | 是 | 是 | 【未确认】 | 否 |
| 按 maxthreads 并发 | 是 | **否（串行）** | 是 | 是 | 声称多线程 | 否 |
| 传 ssid/sspassword | 是 | 是（可选） | 是 | 是 | 【未确认】 | **否（永远匿名）** |
| dev 凭据形态 | unMagic 混淆常量（在仓库） | XOR 混淆常量（在仓库） | **编译期宏（不在仓库）** | XOR 混淆常量 | 闭源 | XOR 混淆常量 |
| mix 合成图 | 本地（artwork.xml 图层引擎） | **本地**（MiximageGenerator） | SS 服务端（1 图 = 3 配额） | 【未确认】 | 客户端 XML 描述符引擎 | 无 |
| **持久化结果缓存** | **有（db.xml + quickid.xml + 媒体目录）** | **无**（仅 TGDB 三张查表） | 【未核查】 | 【未核查】 | **有**（download once, scrap many） | **无** |
| 字段级多源优先级 | **有（priorities.xml）** | 无（单源） | 无 | 无 | 无 | 无 |

> **结论**：**只有 Skyscraper 同时具备「多源 + 字段级优先级 + 持久化缓存 + 离线再生成」四项能力**，是自建工具最值得对标的架构。

---

## 12. 媒体资源的版权与缓存注意事项

### 12.1 各源的「缓存 / 再分发」条款汇总

| 数据源 | 明示许可 | 允许本地缓存 | 允许再分发 | 署名要求 | 商业使用 |
|---|---|---|---|---|---|
| **MAME `/hash`** | **CC0-1.0**（`COPYING` + 每文件 SPDX 头） | ✅ | ✅ 无条件 | 无 | ✅ 无条件 |
| **libretro-database** | **CC BY-SA 4.0**（LICENSE 文件） | ✅ | ✅ 但 ShareAlike 传染 | **必须** | ⚠️ 需同许可开放衍生 |
| **ScreenScraper** | **CC BY-NC-SA 4.0**（全站页脚 `rel="license"`） | ✅ | ✅ 但 **NC + SA** | **必须** | ❌ API 条款另限「完全免费的应用」 |
| **IGDB** | FAQ 明示允许存储；引用 Twitch 开发者协议 | ✅ 官方鼓励 | ❌ Twitch 协议禁止 re-syndication | 商业需**用户可见**署名 | ✅ 免费（需签合作协议） |
| **MobyGames** | 订阅条款 | ✅ 官方推荐 | ❌ 禁止 repackage/resell | **必须**（"Data by MobyGames.com"） | 仅 Bronze($99.99/mo) 及以上 |
| **ArcadeDB** | 无正式许可；自称 Fair Use (17 U.S.C. §107) | ✅ | 未明示 | **必须**（"Scraping using Arcade Database by motoschifo"） | 未明示 |
| **Redump DAT** | 无明示（robots 全放行） | ✅ | 🟡 事实容忍 | 无要求 | 🟡 |
| **No-Intro DAT** | 无明示（含商标 + 反盗版声明） | ✅ | 🟡 事实容忍（libretro 长期再分发） | 无要求 | 🟡 |
| **LaunchBox** | **无正式条款**，仅作者论坛默许 | ✅ | 🟠 **图片勿再分发** | 无要求 | 🟠 |
| **TheGamesDB** | **无任何条款页（法律真空）** | 🟠 | 🟠 无授权 | 无要求 | 🟠 |
| **libretro-thumbnails** | **无 LICENSE 文件** | 🟠 | 🔴 勿整包再分发 | 无要求 | 🔴 |
| **TOSEC** | **"All Rights Reserved"** | 🟠 | 🔴 | 无要求 | 🔴 |
| **GiantBomb** | — | — | — | — | **API 已下线** |
| **Wikidata** | **CC0**（结构化数据） | ✅ | ✅ **无条件** | **无** | ✅ **无条件** |
| **Bangumi** | **CC BY-SA**（条目信息）；封面自称 Fair use | ✅ | ⚠️ 文本可（需 SA）；**封面不建议** | **必须**（文本） | ⚠️ 站点素材「不可用于商业目的」 |
| **VNDB** | **ODbL + DbCL**；图/简介「may be subject to separate license conditions」 | ✅ | ⚠️ 需 SA + 保持开放 | **必须** | ❌ API **仅非商业免费** |
| **zh.wikipedia 文本** | CC BY-SA 4.0 | ✅ | ✅ 需署名 + SA | **必须** | ✅ |
| **zh.wikipedia 封面图** | **NonFree / Fair use** | 🔴 | ❌ 只能在该条目上下文使用 | — | ❌ |
| **Steam appdetails** | **无条款页（灰色）** | 🟠 | 🟠 | — | 🟠 |

图例：✅ 明确允许 ／ ❌ 明确禁止 ／ 🟡 无条款但有长期事实容忍 ／ 🟠 无条款、风险中等 ／ 🔴 高风险

### 12.2 六条实操准则

1. **区分「元数据文本」与「美术资源」的风险等级。**
   **【事实】** 哈希/文件名/发行年份这类事实性数据，各 DAT 项目普遍容忍再分发（libretro 就在做）。**【推断】** 而封面扫描图、截图、视频的著作权属于**原发行商**，任何数据源（包括 ScreenScraper 的 CC BY-NC-SA、libretro-thumbnails）都无权把它们授权给你 —— 这些源自己也只是在 fair use / 事实容忍下运作。libretro-thumbnails README 就直说："*The game art itself and promotional art originates from the work of each respective game's developers and publishers.*"

2. **「本机缓存自用」与「云端集中缓存对外服务」是完全不同的法律处境。**
   前者在所有源下都是被鼓励的（IGDB "we prefer if you store and serve the data"、MobyGames "That's what we recommend"、Skyscraper 整个架构就建立在这上面）。后者会同时触发 ScreenScraper 的 NC 条款、Twitch 的 re-syndication 禁令、MobyGames 的 repackage 禁令。

3. **CC BY-NC-SA（ScreenScraper）的 SA 传染性容易被忽略。** 若你的产品**内嵌**了 ScreenScraper 的数据/媒体并对外分发，衍生作品也须以 BY-NC-SA 发布。**【推断】** 只有「运行时由用户自己的账号拉取到用户自己机器上」才能干净地绕开这一点 —— 这也是 Skyscraper/Skraper/ES-DE 都要求用户填自己 SS 账号的深层原因。

4. **libretro-database（CC BY-SA）与 MAME hash（CC0）的差别，对闭源产品是决定性的。** 需要哈希库且不想承担 ShareAlike 义务，就用 **MAME `/hash`**（代价：只有 CRC+SHA1，且不覆盖街机）。

5. **署名必须做在用户可见的静态位置。** IGDB 原文要求 "visible to your users and **located in a static location** (e.g. not in a change log)"；MobyGames 要求字面文本 "Data by MobyGames.com"；ArcadeDB 要求 "Scraping using Arcade Database by motoschifo"。**【推断】** 一个「关于/数据来源」页面同时列出所有源，是成本最低的合规做法。

6. **利用各源自带的增量/条件请求机制，本身就是合规姿态。**
   - ScreenScraper `mediaJeu.php` 传本地图片的 crc/md5/sha1 → 命中则只回 `CRCOK`，零带宽
   - TGDB `/v1/Games/Updates?last_edit_id=` 增量拉取
   - IGDB Data Partner 的 24h CSV dumps
   - libretro-thumbnails 的 `?nocaches=CURRENTDATE`
   - LaunchBox / auto-datfile-generator 的 24h 全量包 + `Last-Modified` 头

### 12.3 IGDB 图片 30 天过期的实际影响

**【事实】** IGDB 原文："Images that are removed or replaced from IGDB.com exist for **30 days** before they are removed. Keep that in mind when designing cache logic."
**【推断】** 这意味着若只缓存图片 URL 而不缓存图片字节，超过 30 天的链接可能失效。但缓存图片字节又与 Twitch 协议的 24 小时缓存条款冲突。**这是 IGDB 作为媒体源的结构性矛盾** —— 也印证了 Skyscraper 作者选择「IGDB 只取文本、不取媒体」的判断是合理的。

---

## 13. 对自建刮削工具的可执行建议

> 本节区分 **【事实依据】**（前文已给出一手来源）与 **【推断/设计判断】**（我基于事实做出的建议，不是任何一手来源的原话）。

### 13.1 推荐的数据源优先级链

#### 第 0 层：离线哈希识别（不打任何在线 API）

**【推断】** 这是整个架构的地基。先用本地 DAT 把「ROM 文件 → 规范游戏名 + 平台 + 区域」确定下来，在线 API 只用来补元数据和媒体。

| 用途 | 首选 | 理由（事实依据） |
|---|---|---|
| 卡带类哈希 | **libretro-database `/metadat/no-intro/`** | crc+md5+sha1+serial 齐全；纯文本 clrmamepro 格式好解析；CC BY-SA 4.0（§5.1） |
| 光盘类哈希 + serial | **libretro-database `/metadat/redump/`** | libretro 版**补入了 serial**，官方 Redump DAT 反而没有（§8.2） |
| 需要 SHA256 / 最新精度 | **auto-datfile-generator 的 `no-intro.zip`** | 334 个原始 Logiqx DAT，含 `sha256`；GitHub CDN 每日重建（§8.1） |
| 闭源商业产品的零风险兜底 | **MAME `/hash`** | **CC0-1.0 公有领域**，无署名义务、无 ShareAlike（§8.4）。代价：只有 CRC+SHA1，且不含街机 |
| 街机 | libretro `/metadat/mame/` 或 MAME `-listxml` | MAME `/hash` 不含街机 ROM 哈希（§8.4） |

**【推断】铁律：绝不直接抓 `datomatic.no-intro.org`。** 单次参数畸形的请求就会导致**永久 IP 封禁**且需人工发邮件解除（§8.1 有实测证据）。走 GitHub 镜像。

#### 第 1 层：在线元数据 + 媒体

**【推断】** 按「哈希命中率 × 媒体丰富度 × 条款友好度」排序：

```
1. ScreenScraper       ← 唯一以 hash+size 为强制主键的源，媒体最全（wheel/bezel/manual/3D box）
   ↓ 未命中 / 缺字段
2. IGDB                ← 文本质量最好，商业免费，有中文别名；但不提供按平台区分的美术
   ↓ 缺封面
3. TheGamesDB          ← 免费、有 ByGameHash 端点、图片 CDN 尺寸齐全；但无任何条款（法律真空）
   ↓ 街机专用
4. ArcadeDB            ← 免 key、媒体极全（bezel/cpanel/pcb/flyer/manual）；单 IP 单线程
   ↓ 离线兜底
5. LaunchBox Metadata.zip + libretro-thumbnails
                       ← 完全离线，102 MB 包含 18.7 万条 Game + 132 万条图片索引
```

**理由（均为事实依据）**：
- ScreenScraper 优先：官方要求「除非获得豁免，你**必须**发送 crc/md5/sha1 之一**以及**文件大小」（§1.4）—— 这意味着它的匹配是**基于内容而非文件名**，误匹配率结构性地低于其他源。Skyscraper 官方也建议 "scrape your roms with `screenscraper` first, and then use `thegamesdb` to fill out the gaps"（§11.5）。
- **【推断】** 但 ScreenScraper 有两个硬约束：devid 需论坛人工申请（§1.1，实测无 devid 直接 403）、API 只允许集成进「完全免费的应用」（§1.2）。**若你的工具计划商业化，ScreenScraper 从第 1 位降到不可用**，此时 IGDB 升为首选。
- IGDB 不做媒体：Skyscraper 作者的判断是「IGDB 不区分平台版本的美术资源」（§11.5），加上 IGDB 图片 30 天过期与 Twitch 协议 24 小时缓存条款的矛盾（§12.3）。
- GiantBomb **已整体下线**，不要纳入设计（§7）。
- MobyGames 只在「其他源全都没有」时用：官方限速 720 次/小时且 Skyscraper 记录其限制是「全体用户共享」（§6.2）；Hobbyist 档 $9.99/月还仅限非商业。

#### 第 2 层：中文标题与中文简介（见 §9、§10）

**【推断】** 这一层必须与第 1 层解耦，因为没有任何一个源同时具备「高 ROM 覆盖」+「高中文覆盖」，且 **§9 已确认：没有任何中文源支持 ROM 哈希查询**。

```
ROM 哈希 ─(DAT)→ 规范英文/日文名 + 平台 + 年份
                          │
                          ├─(名称+平台+年份 模糊匹配 + 置信度校验)→ Bangumi   ← 中文名/中文简介质量最高
                          │
                          └─(名称+平台匹配)→ Wikidata ─(P5794/P1733/P11688/P5732)→ IGDB / Steam / MobyGames / Bangumi
                                                ↑
                                    Wikidata 是跨库 ID 的枢纽（CC0，法律最干净）
```

**推荐的中文层组合（【推断】）**：

| 角色 | 来源 | 依据 |
|---|---|---|
| **中文基座** | **Bangumi Archive dump**（`subject.jsonlines` 按 `type==4` 过滤 ≈ 87k 条） | `name_cn` 是一级字段；实测欧美/日系主机游戏均有中文译名（§9.1） |
| **infobox 解析** | 官方 `wiki-parser-go` / `wiki-parser-py` | dump 的 `infobox` 是原始 wiki 字符串，平台/发行日期/开发商都在里面（§9.1） |
| **封面补齐** | Bangumi 在线 API `/v0/subjects/{id}` 或 `lain.bgm.tv` CDN | **dump 完全没有图片字段**（§9.1）；CDN 无防盗链、`immutable` |
| **跨库 ID 桥接 + 第二中文名源** | **Wikidata dump（CC0）** | IGDB 覆盖 80.8%、Steam 70.8%、Bangumi 3.1%；zh 标签覆盖 27–42%（§10.2/§10.3） |
| **PC 中文简介** | Steam `appdetails?l=schinese` | 中文简介近 100%；appid 从 Wikidata 的 124,464 个 `P1733` 反查（§9.3） |
| **galgame 官方中文名交叉验证** | VNDB dump（ODbL） | `titles[].lang=zh-Hans/zh-Hant` 且 `official=true`（§9.2） |

**【推断】明确排除**：GameFAQs（反爬 + 条款不可取证）、机核（无库 + 条款明禁再发布）、豆瓣（API 已死）、游民星空游戏库（**robots.txt 明确封禁 ClaudeBot**）、3DM/A9VG（无库无 API）。巴哈姆特 acg 站仅作繁中最后手段。

**【推断】匹配置信度是这一层的核心风险。** Bangumi 搜索是模糊最近邻，实测 `Solar Jetman` 会错配到 `Solar 2`。必须做三重校验（平台一致 + 年份差 ≤1 + 名称相似度阈值），宁可留空也不要写错的中文名。

**【事实】** Wikidata 是唯一同时持有各库 ID 与中文标签的公共数据源（实测 2026-08-30，`?g wdt:P31 wd:Q7889`，共 175,739 条）：

| 属性 | 名称 | 覆盖条目数 | 占比 |
|---|---|---|---|
| **P5794** | Internet Game Database game ID（IGDB slug） | **142,011** | **80.8%** |
| P1733 | Steam application ID | 124,464 | 70.8% |
| P11688 | MobyGames game ID | 64,594 | 36.8% |
| P4769 | GameFAQs game ID | 17,256 | 9.8% |
| **P5732** | **Bangumi subject ID** | 5,410 | 3.1% |
| P7622 | TheGamesDB game ID | 1,442 | 0.8% |

其他可用属性：`P9043` IGDB numeric game ID、`P5795` IGDB platform ID、`P9650` IGDB company ID、`P1933` MobyGames game ID (former scheme)、`P5868` MobyGames platform ID、`P7623/P7634/P7642` TheGamesDB platform/developer/publisher ID、`P6444` 豆瓣游戏 ID、`P400` platform、`P178` developer、`P123` publisher、`P577` publication date、`P136` genre、`P18` image。

**【事实】中文标签覆盖率实测**（按平台，`?g wdt:P31 wd:Q7889 ; wdt:P400 <平台>`，统计有 `rdfs:label` 且 `LANG=zh` 的条目）：

| 平台 | Wikidata 条目数 | 有 zh 标签 | 覆盖率 |
|---|---|---|---|
| NES (Q172742) | 1,205 | 461 | **38.3%** |
| SNES (Q183259) | 1,424 | 449 | **31.5%** |
| Nintendo Switch (Q19610114) | 7,148 | 1,966 | **27.5%** |

> **【推断】** 中文标签覆盖率稳定在 **27–38%**，与平台年代无关。**Wikidata 单独用不够，必须与 Bangumi 组合。**

**【事实】可直接使用的 SPARQL 查询模板**（已实测跑通）：
```sparql
# 按平台取「英文名 + 中文名 + 各库外部 ID」
SELECT ?game ?enLabel ?zhLabel ?igdb ?moby ?bgm ?steam WHERE {
  ?game wdt:P31 wd:Q7889 ; wdt:P400 wd:Q183259 .      # Q183259 = SNES
  OPTIONAL { ?game rdfs:label ?enLabel . FILTER(LANG(?enLabel)="en") }
  OPTIONAL { ?game rdfs:label ?zhLabel . FILTER(LANG(?zhLabel)="zh") }
  OPTIONAL { ?game wdt:P5794  ?igdb  }
  OPTIONAL { ?game wdt:P11688 ?moby  }
  OPTIONAL { ?game wdt:P5732  ?bgm   }
  OPTIONAL { ?game wdt:P1733  ?steam }
}
```
别名用 `skos:altLabel`（同样 `FILTER(LANG(?x)="zh")`），可拿到中文别名/俗称。

**【事实】必须按平台/年份分片查询。** 实测**全库范围的 zh 标签 COUNT 查询直接超时**：
```sparql
SELECT (COUNT(DISTINCT ?g) AS ?c) WHERE { ?g wdt:P31 wd:Q7889 ; rdfs:label ?l . FILTER(LANG(?l)="zh") }
→ 超时；加 wdt:P5794 约束后仍 HTTP 504
```
**【事实】WDQS 官方限制**（<https://www.mediawiki.org/wiki/Wikidata_Query_Service/User_Manual>）：
> "There is a hard query deadline configured which is set to **60 seconds**."
> "access to the service is limited to **5 parallel queries per IP**"
> "One client is allowed **60 seconds of processing time each 60 seconds**"
> "One client is allowed **30 error queries per minute**"（超出 → HTTP 429 + `Retry-After`）
> "Clients who don't comply with the **User-Agent policy** may be blocked completely"

> **【推断】** 结论：**全量中文映射表必须走 Wikidata dump 离线构建**（<https://dumps.wikimedia.org/wikidatawiki/entities/>，CC0），SPARQL 端点只用于增量补漏和交互式验证。

### 13.2 离线优先的可行性 —— **可行，且应该这么做**

**【推断】** 一套完全不依赖任何在线 API 的最小可用系统是存在的，全部组件都可下载：

| 层 | 组件 | 体积 | 更新频率 | 许可 |
|---|---|---|---|---|
| 哈希识别 | libretro-database `/metadat` | 数百 MB（git） | 持续 | CC BY-SA 4.0 |
| 哈希识别（零风险） | MAME `/hash` | 数十 MB | 随 MAME 发布 | **CC0** |
| 富文本元数据 | LaunchBox `Metadata.zip` | 102 MB（解压 544 MB） | **每日** | 无条款（作者默许） |
| 封面/截图/标题画面/Logo | libretro-thumbnails 服务器 | 按需拉取 | 约 2 天 | **无 LICENSE（高风险）** |
| 中文标题 + 跨库 ID | Wikidata dump | 最大 253 GB | 每日 | **CC0** |
| 中文标题 + 中文简介 | **Bangumi Archive dump**（§9.1） | 435 MB zip（解压 ~1.8 GB） | **每周三** | CC BY-SA（仓库无 LICENSE） |
| galgame 官方中文标题 | VNDB db dump（§9.2） | 188 MB | 每日 | ODbL（**仅非商业免费**） |

**【推断】** 这条路线的价值：
1. **零配额压力** —— 不受 ScreenScraper 20k/天、TGDB 月度 allowance、IGDB 4 req/s 的任何约束；
2. **可重复构建** —— 换一套美术风格、改一次命名规则，不需要重新打任何 API（这正是 Skyscraper 两阶段架构的核心动机，§11.1）；
3. **法务上更干净** —— MAME hash (CC0) + Wikidata (CC0) 组合可无条件商业使用。

**【推断】** 但离线路线的**真实缺口**是：
- **媒体**。libretro-thumbnails 只有 4 类图（boxart/logo/snap/title），没有 video、没有 marquee、没有 bezel、没有 3D box、没有 manual。这些**只有 ScreenScraper 有**（§1.5）。若这些是产品必需，就无法纯离线。
- **较新的游戏**。LaunchBox / libretro DAT 对最近几年的作品覆盖弱于 IGDB。
- **中文简介**。Wikidata 只有标签没有简介；能提供中文简介的只有 Bangumi 和 ScreenScraper 的 `synopsis_zh`（§9）。

> **【推断】设计判断**：做「**离线优先 + 在线补充**」的混合架构 —— 离线层保证 100% 的 ROM 都有一个可用的规范名与基础元数据，在线层只负责「离线层未覆盖的字段」。这与 Skyscraper 的 `--cache report:missing=<type>` + `--fromfile` 工作流（§11.1）是同一个思路，且它已经被验证是配额友好的。

### 13.3 必须做的本地缓存设计

**【推断】** 下面每一条都对应 Skyscraper / Skraper 中已被验证的设计，标注了出处。

#### (1) 两阶段分离：采集 ≠ 生成

对标 Skyscraper 的 `-s <MODULE>`（采集）与 省略 `-s`（生成）。**任何在线请求的产物必须先落地到缓存，生成前端产物是一个纯离线步骤。** 这样才能在不重新打 API 的前提下重跑生成。（§11.1）

#### (2) 三个 ID 概念必须分开，不能混用

**【推断】** 这是最容易做错的地方：

| 概念 | 用途 | 计算方式 | 参考实现 |
|---|---|---|---|
| **`cacheId`（缓存主键）** | 本地缓存索引 | ROM 内容 SHA1；对 zip/7z/cue/gdi 等「不稳定」容器与 >50 MB 文件退化为**文件名** SHA1 | Skyscraper `NameTools::getCacheId()`（§11.1） |
| **查询哈希（crc/md5/sha1）** | 发给远端 API | **压缩包必须解压后再算**（DAT 里的哈希针对解压内容） | Skyscraper `getSearchNames()` 用 `7z x -so`（§1.4） |
| **`gameId`（游戏实体 ID）** | 跨源合并的锚点 | 各源自己的数字/字符串 ID | ScreenScraper `jeu.id`、IGDB slug、Wikidata QID |

同一个 ROM 会有 3 个不同的标识符，混用会导致缓存命中率崩塌或误合并。

#### (3) quickid 层：文件路径 + mtime → cacheId

**【事实】** Skyscraper 的做法（§11.1）：
```xml
<quickids>
  <quickid filepath="<绝对路径>" timestamp="<文件 mtime 毫秒>" id="<cacheId>"/>
</quickids>
```
```cpp
if (quickIds.contains(path) && info.lastModified().toMSecsSinceEpoch() <= quickIds[path].first)
    return quickIds[path].second;   // 命中：完全不读文件
```
**【推断】** 没有这一层，每次运行都要对全部 ROM 重算 SHA1（大库上是分钟级到小时级的差别）。用 mtime 做失效判定足够 —— ROM 一旦被替换，mtime 必然变新。

#### (4) 资源表的去重键必须是三元组，而不是二元组

**【事实】** Skyscraper：`(cacheId, type, source)`（§11.1）。即「每个游戏 × 每种资源 × 每个源」各存一份，**并存而非覆盖**。
**【推断】** 这是多源合并能工作的前提 —— 如果后写的源覆盖先写的，就永远无法做「字段级 fallback」，也无法在不重新联网的情况下调整优先级。

记录结构建议（对标 Skyscraper `db.xml`）：
```
resource(cacheId, type, source, timestamp, value)
```
`timestamp` 必须存，因为**未在优先级表中列出的源要按时间戳排序**（新的优先）。

#### (5) 字段级（而非源级）优先级表

**【事实】** Skyscraper `priorities.xml` 的模板证明不同字段的最佳源不同（§11.1）：
```xml
<order type="title">     import > esgamelist > openretro > arcadedb > screenscraper > mobygames > thegamesdb
<order type="platform">  screenscraper > thegamesdb > mobygames
<order type="developer"> import > esgamelist > arcadedb > mobygames
```
**【推断】** 关键设计点：
- **本地/人工来源永远排最前**（`import`/`esgamelist`），保证用户手工修正不被在线数据覆盖 —— Skyscraper 文档明说手工添加的资源 "will be prioritized above all others"；
- 优先级表**按平台可覆写**（Skyscraper 是每平台一份 `priorities.xml`）；
- **未列出的源不报错，按时间戳兜底**，这样加新源时不必修改配置。

**【推断】中文项目的补充**：应为 `title_zh` / `description_zh` 单独设优先级链：
```xml
<order type="title_zh">
  <source>manual</source>          <!-- 人工修正，永不被覆盖 -->
  <source>bangumi</source>         <!-- name_cn 是一级字段，质量最高 -->
  <source>wikidata</source>        <!-- rdfs:label@zh，CC0，覆盖 27–42% -->
  <source>screenscraper</source>   <!-- noms 的中文区标题（覆盖率未验证） -->
  <source>igdb</source>            <!-- alternative_names 中 comment 含 "Chinese" -->
  <source>vndb</source>            <!-- 仅 galgame，titles[].lang=zh-Hans/zh-Hant -->
</order>
<order type="description_zh">
  <source>manual</source>
  <source>bangumi</source>         <!-- summary，实测中文占比 70–86% -->
  <source>steam</source>           <!-- 仅 PC：appdetails?l=schinese，中文简介近 100% -->
  <source>screenscraper</source>   <!-- synopsis_zh -->
  <source>zhwiki</source>          <!-- REST summary，注意默认繁体，需 variant=zh-cn -->
</order>
```

#### (6) 区域链与语言链要分开

**【事实】** Skyscraper 文档的明确区分（§11.1）：
> "Setting a language is a user-preferred thing and will only affect the game descriptions and tags (genres). The remaining game data is tied to the **region** instead (artwork and, in some cases, the game name)."

**【事实】** 且区域应从**文件名自动识别**（`(Europe)`、`(J)`、`(USA)`）并顶到链首，未命中就沿链下移（ES-DE 与 Skyscraper 都这么做，§11.1/§11.2）。
**【推断】中文项目**：语言链设为 `zh → ja → en`，区域链设为 `自动识别 → cn → tw → jp → wor → us → eu`（注意 ScreenScraper 的区域短名是 `cn`/`tw`，语言短名是 `zh`，见 §1.5）。

#### (7) 媒体缓存：按 `<类型>/<源>/<cacheId>` 分层 + 内容哈希去重

**【事实】** Skyscraper 的目录布局（§11.1）：
```
cache/<PLATFORM>/{covers,screenshots,wheels,marquees,videos}/<MODULE>/<cacheId>
```
**【事实】** Skraper 的补充做法："**Hash computation allow media to be saved once, linked many**"（§11.3）—— 用媒体文件自身的哈希做去重，同一张图被多个游戏引用时只存一份。
**【推断】** 两者结合：物理层用「媒体内容哈希 → 文件」做 CAS 存储，逻辑层用 `(cacheId, type, source) → 媒体哈希` 做引用。对系列作/多区域版本能省下大量空间。

#### (8) 用各源的条件请求机制做增量更新

**【事实】** 最有价值的一条：ScreenScraper `mediaJeu.php` 支持传本地已有图片的 `crc`/`md5`/`sha1`，一致时**只返回文本 `CRCOK`/`MD5OK`/`SHA1OK` 而不回传图片**，官方注为 "optimisation des mises à jour"（§1.5）。
其他源：TGDB `/v1/Games/Updates?last_edit_id=`（§2.4）、IGDB Data Partner 的 24h CSV dumps（§3.7）、LaunchBox / auto-datfile-generator 的 `Last-Modified` 头（§4.2/§8.1）、libretro-thumbnails 的 `?nocaches=CURRENTDATE`（§5.2）。

#### (9) 配额管理是**强制**的，且不要硬编码任何数字

**【事实】** ScreenScraper 官方原话：
> "Cette gestion de « Quota » par logiciel est désormais **obligatoire** afin de ne pas saturer nos serveurs pour rien."（§1.3）

**【事实】** 三份官方文档给出三个不同的日配额数字：Recalbox wiki 15,000、Batocera wiki 50,000、Skyscraper 文档 20,000（§11.4）。
**【推断】** 因此必须**从响应里读 `maxrequestsperday` / `maxrequestspermin` / `maxrequestskoperday` / `maxthreads` 动态计算**，而不是写死。Skyscraper 的做法可直接抄：
```cpp
reqRemaining = maxrequestsperday - requeststoday;   // 每次响应都重算
```

**【事实】** 并发数应取自 `ssuserInfos.php` 的 `maxthreads`（Batocera 与 Recalbox 都这么做，ES-DE 不做而选择完全串行，§11.5）。
**【事实】** 状态码处理可直接对标 Recalbox 的实现（§11.4）：
```
429 → sleep 5s 重试（线程超限，可恢复）
430 / 431 → QuotaReached（当日配额耗尽，停止而非重试）
400 / 401 / 403 / 423 / 426 → FatalError（凭据/封禁/服务关闭，立即停止）
404 → NotFound（记录为「已查过但无结果」，不要下次再查）
```

**【推断】特别注意 431（当日未识别 ROM 配额）**：这意味着**盲目重试未命中的 ROM 会额外消耗一份独立配额**。缓存里必须记录「negative result」（查过、没有），并设一个较长的 TTL，避免每次运行都重打同样的空查询。

#### (10) 缓存运维能力清单（对标 Skyscraper）

**【推断】** 以下能力缺一不可，否则缓存会变成无法维护的黑箱（全部对应 §11.1 的 `--cache` 子命令）：

| 能力 | 作用 |
|---|---|
| `show` | 按源 × 类型统计条目数 —— 判断某个源值不值得继续用 |
| `report:missing=<type>` | **导出缺失清单** —— 与下一条组合是配额友好的核心 |
| `fromfile <清单>` | 只处理清单内的 ROM —— 把稀缺配额精确花在缺口上 |
| `edit` | 人工修正，且修正结果优先级最高、不被在线数据覆盖 |
| `purge:m=<源>` / `:t=<类型>` | 某个源的数据质量变差时定向清除，不影响其他源 |
| `vacuum` | 清除与当前 ROM 集合无关联的资源 |
| `validate` | 清理 db 有记录但文件丢失、或文件在但无记录的孤儿 |
| `merge:<目录>` | **跨机器合并缓存** —— 缓存目录必须设计成自包含可移植的 |

**【事实】** Skyscraper 明确保证缓存目录的可移植性："Each subfolder in the cache folder is self-contained and can be copied to other Skyscraper installations at your convenience."（§11.1）

#### (11) 生成前端产物时保留用户拥有的字段

**【事实】** Skyscraper 按前端定义了「元数据保留清单」（<https://github.com/muldjord/skyscraper/blob/master/docs/FRONTENDS.md>）：
- EmulationStation：`favorite`, `hidden`, `playcount`, `lastplayed`, `kidgame`, `sortname`
- Attract-Mode：`cloneof`, `rotation`, `control`, `status`, `displaycount`, `displaytype`, `altromname`, `alttitle`, `extra`, `buttons`

**【事实】** 并提供 `gameListBackup="true"`，生成前自动备份为 `gamelist.xml-yyyyMMdd-hhmmss`。
**【推断】** 收藏、游玩次数、上次游玩时间是**用户产生的数据**，刮削器无权覆盖。这一条如果做错，用户会永久丢数据。

### 13.4 三条必须避开的坑（均有一手证据）

1. **不要直接抓 No-Intro Dat-o-Matic** —— 单次畸形请求即永久封 IP，需人工发邮件解除；其 robots.txt 声称允许抓取，**不可作为安全依据**（§8.1）。改用 GitHub 每日镜像。
2. **不要假设 LaunchBox 能按哈希查** —— `Files.xml` 只有「文件名 → 游戏名」三个字段，全库唯一的 CRC 是图片自身的校验和（§4.4）。必须以文件名为桥接键。
3. **不要假设 Redump 官方 DAT 有 serial** —— 实测全文无 `serial=` 属性（§8.2）。光盘 serial 匹配要用 libretro 的转换版。

### 13.5 关于商业化的分岔

**【推断】** 这是设计前必须先回答的问题，因为它会改变整条优先级链：

| 若工具… | ScreenScraper | libretro-database | 中文来源 | 建议 |
|---|---|---|---|---|
| **完全免费开源，数据留在用户本机** | ✅ 可用（符合 API 条款 + CC NC） | ✅ 可用 | ✅ 可用 | 按 §13.1 的完整链 |
| **商业产品，但只在用户机器上缓存** | ❌ API 条款禁止；CC 的 NC 也禁止 | ⚠️ SA 传染，闭源产品需评估 | 见 §9 | 首选 IGDB（明确允许商业）+ MAME hash (CC0) + Wikidata (CC0) |
| **要分发离线数据包给第三方** | ❌ | ⚠️ 须以 CC BY-SA 发布 | 见 §9 | 只有 **MAME `/hash` (CC0) + Wikidata dump (CC0)** 是干净的；IGDB 须先取 `partner@igdb.com` 书面确认 |

---

### 13.6 三条中文层的专属注意事项

1. **【事实】** Bangumi dump **不含任何图片字段**，封面必须在线补齐（§9.1）。若做纯离线分发，中文封面这一块无解。
2. **【事实】** Wikidata / zh.wikipedia **不能作为封面图源** —— Commons 明确拒绝合理使用内容，游戏封面只在 zh.wikipedia 本地以 `NonFree = true` 托管，FC 平台 `P18` 覆盖率仅 3.4%（§10.7）。
3. **【事实】** zh.wikipedia REST API **默认返回繁体**，取简体必须显式传 `?variant=zh-cn` 或 `&uselang=zh-cn`（§10.6）。

---

## 14. 来源清单

> 全部为本次调研实际访问过的一手来源。

**ScreenScraper**
- <https://api.screenscraper.fr/webapi2.php>（API v2 完整文档）
- <https://www.screenscraper.fr/membreinscription.php>（注册规则原文）
- <https://www.screenscraper.fr/faq.php>（页脚 CC BY-NC-SA 4.0 声明）

**TheGamesDB**
- <https://api.thegamesdb.net/>、<https://api.thegamesdb.net/spec.yaml>
- <https://api.thegamesdb.net/key.php>、<https://thegamesdb.net/register.php>

**IGDB / Twitch**
- <https://api-docs.igdb.com/>
- <https://dev.twitch.tv/docs/authentication/getting-tokens-oauth/>
- <https://legal.twitch.com/en/legal/developer-agreement/>
- <https://www.igdb.com/games/black-myth-wukong>（中文别名实证）

**LaunchBox**
- <https://gamesdb.launchbox-app.com/Metadata.zip>
- <https://forums.launchbox-app.com/topic/54163-is-there-a-public-way-to-get-images-from-the-launchbox-games-database/>
- <https://forums.launchbox-app.com/topic/35162-any-way-to-download-metadata-manually/>
- <https://forums.launchbox-app.com/topic/52709-launchbox-db-api/>

**libretro**
- <https://github.com/libretro/libretro-database>（README + LICENSE + `/metadat`）
- <https://github.com/libretro/RetroArch/blob/master/libretro-db/README.md>
- <https://github.com/libretro/RetroArch/blob/master/libretro-db/libretrodb.c>
- <https://github.com/libretro-thumbnails/libretro-thumbnails>、<https://thumbnails.libretro.com/>
- <https://github.com/Swordfish90/libretro-sqlite-db>

**MobyGames / GiantBomb**
- <https://www.mobygames.com/info/api/>、<https://www.mobygames.com/api/subscribe/>
- <https://www.giantbomb.com/api/>

**DAT 项目**
- <https://datomatic.no-intro.org/>、<https://wiki.no-intro.org/index.php?title=Database_Navigation_Guide>
- <https://datomatic.no-intro.org/stuff/schema_nointro_datfile_v4.xsd>
- <https://github.com/hugo19941994/auto-datfile-generator>
- <http://redump.org/downloads/>、<http://redump.org/robots.txt>
- <https://www.tosecdev.org/downloads>
- <https://github.com/mamedev/mame/blob/master/COPYING>、<https://github.com/mamedev/mame/blob/master/hash/softwarelist.dtd>

**刮削器实现**
- <https://github.com/muldjord/skyscraper>（`docs/CACHE.md`、`CLIHELP.md`、`CONFIGINI.md`、`SCRAPINGMODULES.md`、`REGIONS.md`、`LANGUAGES.md`、`FRONTENDS.md`、`FAQ.md`；`src/cache.cpp`、`src/nametools.cpp`、`src/screenscraper.cpp`；`cache/priorities.xml.example`）
- <https://gitlab.com/es-de/emulationstation-de>（`USERGUIDE.md`；`es-app/src/scrapers/ScreenScraper.cpp/.h`、`GamesDBJSONScraper*`；`es-core/src/Settings.cpp`、`HttpReq.cpp`、`StringUtil.cpp`）
- <https://www.skraper.net/>、<https://www.skraper.net/locales/en/skraper.json>、<https://www.skraper.net/update/update.json>
- <https://github.com/batocera-linux/batocera-emulationstation>（`es-app/src/scrapers/`）、<https://wiki.batocera.org/scrape_from>
- <https://gitlab.com/recalbox/recalbox-emulationstation>（`es-app/src/scraping/`）、<https://wiki.recalbox.com/en/basic-usage/features/internal-scraper>
- <https://github.com/RetroPie/EmulationStation>、<https://raw.githubusercontent.com/RetroPie/RetroPie-Docs/master/docs/Scraper.md>
- <https://github.com/Universal-Rom-Tools/Universal-XML-Scraper>
- <http://adb.arcadeitalia.net/service_scraper.php>

**中文来源**
- <https://raw.githubusercontent.com/bangumi/api/master/open-api/v0.yaml>
- <https://github.com/bangumi/api/blob/master/docs-raw/user%20agent.md>、`How-to-Auth.md`
- <https://bangumi.tv/about/copyright>、<https://bgm.tv/robots.txt>
- <https://github.com/bangumi/Archive>、<https://github.com/bangumi/wiki-syntax-spec>、<https://github.com/bangumi/common>
- <https://api.vndb.org/kana>、<https://vndb.org/d2>、<https://vndb.org/d14>、<https://vndb.org/d17>、<https://vndb.org/robots.txt>
- <https://store.steampowered.com/api/appdetails>、<https://partner.steamgames.com/doc/webapi/ISteamApps>、<https://steamcommunity.com/dev/apiterms>
- <https://www.gcores.com/robots.txt>、<https://site.gcores.com/policies/terms/>

**维基系**
- <https://query.wikidata.org/sparql>
- <https://www.mediawiki.org/wiki/Wikidata_Query_Service/User_Manual>
- <https://www.mediawiki.org/wiki/Wikimedia_APIs/Rate_limits>、<https://www.mediawiki.org/wiki/API:Etiquette>
- <https://foundation.wikimedia.org/wiki/Policy:Wikimedia_Foundation_User-Agent_Policy>
- <https://www.wikidata.org/wiki/Wikidata:Licensing>、<https://dumps.wikimedia.org/wikidatawiki/entities/>
- <https://commons.wikimedia.org/wiki/Commons:Licensing>、<https://zh.wikipedia.org/w/api.php>

---

## 附录：本次调研中未能从一手来源确认的事项

| # | 事项 | 原因 |
|---|---|---|
| 1 | ScreenScraper 各用户等级对应的具体 threads / requests-per-day 分级表 | `faq.php` 未登录不渲染正文 |
| 2 | ScreenScraper 的中文（zh）简介/标题实际覆盖率 | 需 devid 才能查询（实测无 devid 直接 403） |
| 3 | TheGamesDB public/private key 的具体配额数字；其数据/图片的授权条款 | 官方论坛整站 HTTP Basic Auth 401；官网无任何条款页 |
| 4 | TheGamesDB `ByGameHash` 的实际数据覆盖率 | 需 API key |
| 5 | IGDB 中文标题在全库的覆盖率百分比；"Migration Enums to Tables" 的具体年份 | 需 API key；文档未标年份 |
| 6 | MobyGames 低档位是否实际返回 `alternate_titles`（AKAs 标注为 Silver+ 内容） | 需付费订阅 |
| 7 | GiantBomb 的全部条款与配额 | API 已整体下线，文档页已删除 |
| 8 | No-Intro 下载页是否有 CAPTCHA；Standard vs Parent-Clone 的官方正式定义 | 调研中 IP 被 datomatic 永久封禁；官方 Wiki 无正式文档 |
| 9 | Redump / No-Intro / TOSEC 是否存在未公开的正式数据授权 | 均无条款页 |
| 10 | TOSEC 是否有官方 API 或 GitHub 镜像 | 未找到 |
| 11 | Skraper 是否强制要求 SS 账号；其缓存的落盘格式与位置；对 Attract-Mode 的支持 | 闭源；官网无 docs/FAQ 页（全 404） |
| 12 | Skraper 与 ScreenScraper 团队的正式组织隶属关系 | 只有间接证据 |
| 13 | RetroPie 官方当前明确推荐哪一个 scraper | 三者并列，无排他推荐、无弃用声明 |
| 14 | Batocera 的 HfsDB 是否在正式版启用；Recalbox 的哈希文件大小上限 | wiki 未提及；受编译期宏保护 |
| 15 | Bangumi 个人 Access Token 的申请细节；是否存在未公开的实际速率阈值 | `next.bgm.tv/demo/access-token` HTTP 403（需登录）；官方文档确实无限流描述 |
| 16 | GameFAQs / GameSpot 的 robots.txt、官方 API、服务条款 | 调研环境下域名不可达；Fandom 条款页 HTTP 402 |
| 17 | 豆瓣 API v2 关闭的官方公告与时间 | `developers.douban.com` 不可达（`api.douban.com/v2/game/*` 返回 404 可作旁证） |
| 18 | Steam storefront `appdetails` 的实际限流数值 | Valve 从未文档化该端点 |
