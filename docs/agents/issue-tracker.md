# 工单跟踪器：本地 Markdown

这个仓库的工单与规格（规格也叫 PRD）以 Markdown 文件的形式住在 `.scratch/` 下。没有 git remote，也不需要。

## 约定

- 一个特性一个目录：`.scratch/<feature-slug>/`
- 规格是 `.scratch/<feature-slug>/spec.md`
- 工单一个文件一张：`.scratch/<feature-slug>/issues/<NN>-<slug>.md`，编号从 `01` 起，按依赖顺序排，被阻塞的排在后面。多张工单塞进一个文件是不行的。
- triage 状态记在文件靠前处的 `Status:` 行，取值见 `triage-labels.md`
- 阻塞关系记在 `Blocked by:` 行，列出它依赖的工单编号；无阻塞则写明可立即开工
- 评论与对话追加在文件末尾的 `## Comments` 标题下

## 技能说「发布到工单跟踪器」时

在 `.scratch/<feature-slug>/` 下新建文件，目录不存在就建。

## 技能说「取相关工单」时

读引用路径指向的文件。用户一般会直接给路径或工单编号。

## 前沿

**前沿**是当下可以开工的那批工单：阻塞它的工单全部完成。扫 `.scratch/<feature>/issues/`，挑 `Blocked by:` 里列的工单都已完成的，编号小的优先。纯线性依赖时，前沿就是从上往下。
