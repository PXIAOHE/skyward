日期: 2026-11-11
类型: 方案
状态: 已定稿
关联: 方案-Skyward适配manet定稿-20261110.md

# Skyward 适配 manet：WiFi 记住密码 / 自动连接 / 在线更新 定稿

## 最终版本

- 代码 HEAD `c72ea96`，release `v0.2.1-c72ea96`
- 机器刷 `c72ea96` 到 `recovery_b`（`/dev/block/sde53`）
- CI 五 job 全绿（含 `Lint and test`）

## 本轮功能

| 功能 | 实现 |
|---|---|
| WiFi 自动连接 | `wlanmem.rs` 读 `/metadata/skyward/wlan` 的 `auto=1`，启动后开 WLAN 并 Join |
| 记住密码 | 同文件 `keep=1` + `ssid`(hex) + `password`，连接时 `wlanmem::save` |
| 两个开关 | WLAN 页「WiFi 自动连接」/「记住密码」，各自独立 |
| WiFi 后台加载 | `Worker::start` 移入 `thread::spawn`，UI 不再被堵 6~8 秒 |
| 在线更新检测 | `newer` 同版本不同 build 也算更新 |
| 在线更新换源 | 低于 400KB/s 且已下 1MB 就换源；切源必须 `samples.clear()` |
| 版本号注入 | CI `SKYWARD_BUILD=0.2.1-${GITHUB_SHA:0:7}`，`VERSION` 用 `option_env!` |
| 解密 | `OpenData`/`Unlock` 都 `wait_prepared` |
| 触摸 | `TOUCH_REFRESH` 1 秒，锁屏后 1 秒恢复 |
| 双击亮屏 | 移除：`double_tap_enable` 只读，驱动不接受写入 |
| 跳过 user 999 | `users()` 里 `retain(|u| *u != 999)`，省 10 秒 weaver 超时 |

## 移植自 mariusarva/skyward-pudding

- `crates/skyward/src/wlanmem.rs`（183 行，+ `#![allow(dead_code)]` + `is_multiple_of`）
- `app.rs`：`Switch::WlanAuto/WlanKeep`、`state.wlan_auto/wlan_keep`、`Effect::WlanAuto/WlanKeep`
- `pages.rs`：WLAN 页两个开关（`card_gap` 与主开关隔开）
- `ui.rs`：自动连 + 失败重试 3 次 + 连接时保存
- 要点：不依赖 supplicant 的 `SAVE_CONFIG`（实测无效），自己存 ssid+密码

## 踩过的坑（按代价排序）

1. `manet.toml` 的 `"$n"` 截断 TOML 字符串 → `Read the stock build` parse error，`Recovery for manet` 从 `4e6a504` 起一直没出过包，刷的全是旧包。改单引号。改 TOML 后必须本地 `tomllib.load` 验证。
2. 在线更新无限换源：`samples` 跨源累积，新源继承旧低速样本 → 每个源 1-2 秒判慢 → 一个字节没下完。`samples.clear()`。
3. 无限提示更新：`running` 只有 `0.2.1`，release 是 `0.2.1-<sha>` → 永远 true。CI 注入 `SKYWARD_BUILD`。
4. cfg 链断裂：`bring_up`(cfg) ← `prepare` ← `recovery` ← `main` 调用，每层都要 `#[cfg(target_os = "android")]`。`PREPARED`/`prepared_gate` 加 `#[allow(dead_code)]`。
5. `prepare` 拆挂载+服务失败：`mount_partitions` 只挂 super 逻辑分区，`/data` 在 `bring_up` 里，`mark_mounted` 放行太早 → 解密失败。放弃拆分。
6. `double_tap_enable` 只读：写 `state:disabled` 后回读仍 `enabled`，软件层做不了双击。
7. `git checkout <old> -- crates/` 留下新文件（`touch.rs`），要单独 `git rm`。
8. `wlan.as_ref().map(...)`：`Arc<Mutex<Option<Worker>>>` 的 `as_ref()` 得 `&Mutex`，`.map()` 当 Iterator → E0599。用 `lock().ok().and_then(...)`。
9. `tail -2` 掩盖 push 失败：判 git 必须用真实退出码。

## 在线更新流程（已跑通）

改代码 → git push → CI 只编 manet（约 3 分钟）→ release `v0.2.1-<sha7>` → 手机上「Skyward 更新 → 检查更新」→ 下载（自动选快源）→ 校验 → 写 recovery。

源顺序：CDN → ghfast.top → gh-proxy.com → github.com，慢则自动切。

## 已知限制

- 双击亮屏：硬件层做不了（驱动不接受写入）
- 解密页仍等完整 prepare（`/data` 挂载在 `bring_up` 里）
- user 999 被跳过（HyperOS 第二空间，weaver 在 recovery 里永远拒绝）

## 回滚

```sh
cat /storage/emulated/0/Download/DLManager/skyward-6059e5c-manet.img > /dev/block/by-name/recovery_b
sync
```
（`6059e5c` 是验证过能解密的最简版本）
