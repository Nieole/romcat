# ROM 元数据自动化

把一批以 Pegasus 方式维护的模拟器资源，从手工维护变成自动识别、自动刮削、可在主流前端格式间互转的库。

**主库只读。** 那是 10TB 不可再生的资源。工具只写元数据文件、媒体目录与子库，ROM 文件一个字节都不改（ADR-0004）。

## Agent skills

### 工单

建工单、读工单、写规格的位置与约定。见 `docs/agents/issue-tracker.md`。

### Triage 标签

五个角色的标签字符串，以及它们在本地 Markdown 跟踪器里记在哪。见 `docs/agents/triage-labels.md`。

### 领域文档

动手前先读词表与 ADR，输出用词表的词。见 `docs/agents/domain.md`。
