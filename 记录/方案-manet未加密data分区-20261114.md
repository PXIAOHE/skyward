# manet 未整体加密 data 分区的打开方式（PR #1）

日期：2026-11-14
类型：方案
状态：已合入 main，CI 五绿
关联：PR #1（jumpdang）、commit `1c8b94e`、release `v0.2.1-1c8b94e`

## 缘起

`jumpdang` 提了 PR #1：他的 manet 上 recovery 打不开 `/data`。原因不是解密链，而是
fstab 的写法不同——分区只写了 `keydirectory=`，没有 `metadata_encryption=`，也就是
**整个分区没有做 metadata 加密**，但旧代码一见到这种 fstab 就直接报错退出。

## 改动（只动 `crates/skyward-crypto/src/data.rs`，+38/−12）

| 位置 | 改动 |
|---|---|
| `Encryption::of()` | 无 `metadata_encryption=` 但有 `keydirectory=` 时，按 `aes-xts-plain64`、非 wrapped 处理，不再直接 `bail` |
| `map()` | 先探测裸设备上的文件系统；认得出就直接挂，不建 `dm-default-key` 层 |
| `open()` / `unmap()` | 先用 `dm::status()` 确认设备存在再删，避免对不存在的设备动刀 |
| `open_without_credential()` | 没有 `/data/unencrypted/mode` 说明也没有 FBE，直接 bind 用户存储、不标记用户未打开 |

判据是「裸设备上有可识别的文件系统」→ 前面没有加密层；有加密时探测失败，走原路径。

## 合并过程

PR 来自 fork，**GitHub 不给 fork PR 跑 CI**（`check_runs` 为空）。所以：

1. `git fetch https://github.com/jumpdang/skyward.git main:pr1-check`
2. 核净改动：`git diff --stat main..pr1-check` → 只有 `data.rs` 一个文件
3. 核依赖：`skyward_block::probe::filesystem`、`skyward_block::dm::status` 在现有代码里都存在
4. 把 `pr1-check` 推到我们仓库跑 CI → 五绿
5. squash 合并到 main（`1c8b94e`），删掉临时分支

**fork PR 一律先推临时分支跑 CI 再合**，这条以后照办。

## 对本机的影响

无。本机 manet 的 `/data` 真做了 metadata 加密，裸分区探测会失败 → 走原来的
`dm-default-key` 路径，行为不变。改动只对「明文 f2fs 的移植 ROM」生效。

## 备注

- `debug` 分支仍停在浏览器那摊工作（`a8ab28e`），未合并、未清理
- 浏览器方向（NetSurf framebuffer 前端）已验证到「能渲染真网页、能滚动」，
  卡在整机流畅度，暂时搁置
