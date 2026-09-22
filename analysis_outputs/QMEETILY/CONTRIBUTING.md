# Contributing to QMeetily

> QMeetily 是与父项目 meetily 完全独立的子项目。**所有 QMeetily 的改动只影响 `analysis_outputs/QMEETILY/` 子目录,不触碰父项目的任何文件**。

## 分支策略

| 分支前缀 | 用途 | 基准 |
|---|---|---|
| `qmeetily-feat/*` | 新功能 | `devtest` |
| `qmeetily-fix/*` | Bug fix | `devtest` |
| `qmeetily-refactor/*` | 重构(无功能变化) | `devtest` |
| `qmeetily-chore/*` | 杂项(CI、依赖、文档) | `devtest` |
| `qmeetily-hotfix/*` | 紧急修复 | `main` |

**禁止** 使用 `feature/*`、`fix/*` 等不带 `qmeetily-` 前缀的命名 —— 那是父项目 meetily 的命名空间。

## Commit 规范

```
<type>(<scope>): <subject>

# 类型: feat | fix | refactor | docs | chore | test | perf
# 示例:
feat(p0): implement FTS5 search triggers
fix(state): extract state_machine module
chore(ci): add QMeetily workflow stub
```

## Commit Scope 限定(关键)

父仓库当前可能有未提交的 dirty 改动。**任何 QMeetily commit 必须严格 scope 限定**:

```bash
# ✅ 正确
git add analysis_outputs/QMEETILY/CONTRIBUTING.md
git add analysis_outputs/QMEETILY/.github/workflows/qmeetily-ci.yml

# ❌ 禁止
git add .
git add -A
```

提交前必须用 `git status` 确认本次 commit 只包含 QMEETILY 路径下的文件。

## PR 流程

每个 PR 独立分支,完成后请发起 PR 评审。当前阶段为 Stage 0 基建期,只允许合并以下三类 PR:

1. CI/工具链相关
2. 不触碰 `crates/llama-helper/`、`crates/qmeetily-app/src/summary_engine/`、`patches/` 的重构
3. 文档

PR #0.3 引入 `tauri-specta` 后,所有新加 Tauri 命令必须带 `#[specta::specta]` 标注。

## 版本

当前: **v0.1.0-dev**
目标: **v0.2.0**(约 5-6 周后)

## 详细流程手册

完整 PR 拆分、CI 配置、评审清单、发布流程见 `docs/DEVELOPMENT_PROCESS.md`(在 v0.2.0 PR #4.1 引入)。
