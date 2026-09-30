# 09: 在真掌机上核实 ES-DE 读哪里

**What to build:** **人做的一步**：在一台装着 ES-DE 2.x 的掌机上核实——ES-DE 默认从哪里读 `gamelist.xml`、它的数据目录在卡上还是内置存储、`LegacyGamelistFileLocation` 开没开、开了之后读不读平台目录旁那一份。
结论写进这张票末尾，并补进设备档案里 ES-DE 那一段的说明。10 照结论做。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q945`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 记下掌机型号与 ES-DE 版本
  - **没上真机。** 拿主意的人 2026-10-01 裁定：这件事是 ES-DE 的公开行为，改查官方文档与源码，不拿设备核。所以没有「掌机型号」；版本取的是 ES-DE `master`（`4996409dadd7314d9940963432a4d29fa7994996`，`CHANGELOG.md` 顶上是 **3.5.0**）。
  - ⚠️ **票面「ES-DE 2.x」写错了**：2.x 还叫 EmulationStation Desktop Edition，没有 Android 版，数据目录是 `~/.emulationstation`；**3.0 起**加了 Android、改名 ES-DE，数据目录改名 `ES-DE`（`CHANGELOG.md` 3.0：「Renamed the application data directory from .emulationstation to ES-DE」）。掌机上跑的（Android 与 Linux 掌机）都是 3.x。票 10 的「默认照 ES-DE 2.x」按 3.x 读。
- [x] 记下默认读 gamelist 的位置（相对卡根，或在内置存储上）
  - 只有一个位置：`<应用数据目录>/gamelists/<系统名>/gamelist.xml`（`es-app/src/SystemData.cpp` 的 `SystemData::getGamelistPath`：`getAppDataDirectory() + "/gamelists/" + mName`，再接 `/gamelist.xml`）。
  - 应用数据目录（`es-core/src/utils/FileSystemUtil.cpp` 的 `getAppDataDirectory`；`ANDROID.md` 开头配置向导那两段；`USERGUIDE.md`）：
    Android 上是首次启动配置向导里选的那个目录，**默认内置存储根目录下的 `ES-DE`**（`/storage/emulated/0/ES-DE`），也可以选到别处（包括 SD 卡）；
    Linux / macOS 是 `~/ES-DE`（Linux 可用环境变量 `ESDE_APPDATA_DIR` 改）；Windows 是 `C:\Users\<用户>\ES-DE`，便携版是 `ES-DE\ES-DE`。
  - **结论：照 ES-DE 自己的默认，数据目录在内置存储（或家目录）上，不在卡上。** SD 卡上只放 ROM 的掌机，同步目标够不着它。
  - 对 romcat 今天的落点：`gamelists/<平台目录>/gamelist.xml` 相对同步根（卡根）（`grill.md` `Q945` 核查，`crates/core/src/adapter/gamelist.rs` 的 `metadata_path`）。只有应用数据目录**恰好就是卡根**时 ES-DE 才读得到它——照默认设置没人读。
  - 媒体同理：`<应用数据目录>/downloaded_media/`，可由设置 `MediaDirectory` 改到别处（`es-app/src/FileData.cpp` 的 `FileData::getMediaDirectory`）。
- [x] 记下 `LegacyGamelistFileLocation` 的默认值，以及开了之后的行为
  - 默认 **`false`**，是隐藏设置，菜单里没有，只能手改 `es_settings.xml`：`<bool name="LegacyGamelistFileLocation" value="true" />`（`es-core/src/Settings.cpp`：`mBoolMap["LegacyGamelistFileLocation"] = {false, false}`；`FAQ.md`）。
  - 开了：**只有 `<ROM 系统目录>/gamelist.xml` 已经存在时**，读与写都用那一份；不存在时仍读写 `<应用数据目录>/gamelists/<系统名>/`（`getGamelistPath` 只在 `exists(filePath)` 那一支里看这个设置）。
  - 没开而 ROM 系统目录里有 `gamelist.xml`：不读，日志记一条警告叫人挪到 `gamelists/` 下或删掉。2.0.0 起就是这样（`CHANGELOG.md` 2.0.0：「gamelist.xml files are no longer loaded from the ROMs/system directories (although old behavior can be retained via an es_settings.xml option)」）。
  - 仓里早有的调研 `docs/research/metadata-formats.md` 第 10 条说的是同一件事，与源码对得上。
- [x] 结论写回设备档案 ES-DE 那一段的说明
  - **托给票 10**（已在那张票正文追加「收票 09 第 4 条」一行）：设备档案 `crates/core/src/capability/profiles.toml` 是编进二进制的，改它的说明要过门禁，而票 10 本来就要在同一段加「ES-DE 数据目录」一项，一起改。

**拿主意的人 2026-10-01 裁「ES-DE 数据目录」默认值**：**照 ES-DE 自己的默认——在内置存储上，同步目标够不着**。于是默认那一档同步不写 gamelist、差量预览如实说为什么；
数据目录放在卡上的人，在档案里把这一项填成相对卡根的路径（例如 `ES-DE`）。拿主意的人自己几台掌机的数据目录放在哪「不确定、几台不一样」，所以默认不替任何一台猜。
