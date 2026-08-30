# 离线与在线两条刮削路径都要，用策略档案切换

识别与中文名在技术上可以完全离线完成（No-Intro DAT 的 GitHub 每日镜像、Redump DAT、MAME `/hash`、TOSEC、Bangumi 的每周离线 dump），而媒体资源必须上网（Bangumi dump 不含图片，Wikidata 的图片覆盖仅 3.4% 且维基系明确拒绝合理使用内容）。用户要求两条路径都可用且手动可控，因此做成刮削策略档案：预置「离线优先」与「在线优先」两套，在任务级别切换，字段级优先级可单独覆盖。

## Consequences

- **在线优先对这个库有结构性风险**：ScreenScraper 有专门的 431「当日未识别 ROM 配额超限」错误码，配额同时按账号与 IP 计，多账号轮换会导致永久封禁——而汉化版在 ScreenScraper 眼里正是「未识别 ROM」。在线优先档必须默认限流，并把 431 当作硬停止而非重试信号。
- **绝不直连 No-Intro Dat-o-Matic**。调研中一次参数畸形的请求就触发了永久 IP 封禁，其 robots.txt 声称允许抓取但不可作为安全依据。DAT 一律走 GitHub 每日镜像。
- 字段级多源优先级 + 持久化缓存（Skyscraper 的 `priorities.xml` / `db.xml` / `quickid` 那一套）是两个档位共用的机制，不是在线档专有。
- IGDB 存在条款冲突：官方称免费可商用，但其引用的 Twitch 协议只准缓存 24 小时且禁止再分发。做本地缓存需注意这个风险点。

## 修订：Redump 已迁站，指定的镜像指向的是冻结旧站

`redump.org` 是 **2026-06-20 起的冻结镜像**，Redump 已迁至 `redump.info`。旧站 60 个系统，新站 106 个，相差 46 个。

**而 `hugo19941994/auto-datfile-generator` 的 `redump.py` 硬编码了旧域名**——即本 ADR 原文指定的那个 GitHub 每日镜像。结论要拆开：

- **No-Intro 走该镜像仍然正确**，「绝不直连 datomatic」的约束不变。
- **Redump 不能走该镜像**，否则拿到的是冻结旧数据。且旧站会以「系统存在但 No discs found」的形式误导，让人误以为该平台确实未收录。Redump 必须走 `redump.info`。

修正后的实际覆盖：Wii U 541 条、Xbox 360 3707 条、3DO 669 条、Symbian 3 条；**PS Vita 确实没有**。

## 实现坑

**GitHub 的 `/contents` API 在 1000 条处硬截断且不报错。** 调研中因此一度误判 TOSEC 缺少多个平台集；改用 `git/trees?recursive=1` 后才拿到完整的 4502 个 DAT。工具从 GitHub 拉取 DAT 清单时必须避开这个 API。
