日期：2026-11-10
类型：方案
状态：完成（manet 全功能可用）
关联：项目/skyward-probe、devices/manet.toml、PXIAOHE/skyward、K70 Pro（manet / sm8650 / pineapple）

# Skyward 适配 Redmi K70 Pro（manet）—— 定稿方案

## 一、最终成果

镜像：`skyward-dcd158c-manet-OS3.0.306.0.WNMCNXM.img`
sha256：`3ac72069bef88a55898e5d14aa34fc10bbbef5ae56da39df15bc1a0870279608`
刷入：`recovery_b`（`/dev/block/sde53`）

| 功能 | 状态 |
|---|---|
| 解密 data（FBE / NXP eSE weaver） | ✅ |
| WiFi | ✅ 一次点亮 |
| 触摸（含息屏/亮屏后） | ✅ |
| 震动强度调节 | ✅ |
| 双系统 | ✅ |
| 刷机 / 备份 / 终端 / MTP | ✅ |
| U盘 | ❌ Skyward 无 USB host 模式（上游功能缺失） |

## 二、走过的弯路（每条都是实测坑，勿重复踩）

### 1. 模块名必须与实际 .ko 一致

`crate::load` 是 **fail-fast**：`format!("{name}.ko")` 找不到就 `?` 返回，**整个启动链中断**，后面所有 `[[service]]` 一个都不执行。

```
sources: /vendor_dlkm/lib/modules/
实际文件：nxp-nci.ko（连字符！）
错误写法：nxp_nci → 找 nxp_nci.ko → 不存在 → Err → services::start 从不执行
```

`nfc.ko` 也没有独立文件（由 nxp-nci 的依赖带出）。正确列表：

```toml
load = ["goodix_core", "xiaomi_touch", "stm_nfc_i2c", "nxp-nci"]
```

### 2. WLAN 的 fs_ready 必须在「模块加载之后、等待接口之前」

```rust
// ✗ 错：writes 在 if 内 → 第二次点 WiFi 时 if 不执行 → 不写 fs_ready
// ✗ 错：writes 在 if 之后 → 等待循环先跑完，接口永远不出现
// ✓ 对：模块加载 → 写 fs_ready → 等接口
```

### 3. WLAN 要重试 ENODEV，不只是 EBUSY

```
日志实测：the WLAN did not come up: read how wlan0 stands: No such device (os error 19)
```

`/sys/class/net/wlan0` 文件先出现，内核态设备后建立，这一瞬间 `ioctl(SIOCGIFFLAGS)` 返回 **ENODEV(19)**。只重试 EBUSY(16) 无效。

### 4. 触摸：驱动在息屏/亮屏时重置协议状态

```
kernel.log: [GTP-ERR] brl_event_handler: touch head checksum err
            touch_head 00 e9 15 ef 6e b6 51 41     ← 息屏后狂刷
```

小米的 `xiaomi_touch_resume_work` 会把控制器退回上电状态，`enable_touch_raw` 文件不会因此改变，`/dev/input` 也不重新枚举，所以 `input.rs` 的 "panel taken in 时写一次" 覆盖不到。

**解决：上游 PR #30**（`fix/touch-rewritten-while-panel-lit`）—— `TOUCH_REFRESH = 1s` 的后台线程持续重写。

### 5. 解密链移植自 shennong fork（不是自己写）

`xuanwei130-netizen/skyward-for-shennong` 的服务层实现（`create_init_socket` 用 `UnixListener` + 0666 + 清 O_NONBLOCK + `stderr(Stdio::piped())` 把服务错误读进 kmsg）在硬件上验证过。自己手写的 `sockets` 版本（0660 + raw socket + `Stdio::null()`）**解密失败**且看不到错误。

**决定**：整份采用 shennong 的 `crates/`，只保留 manet 的设备参数。

### 6. 解密需要的完整配置（8 个 service + 4 个段）

```toml
# 库搬运 + namespace 隔离关闭（recovery 自带 /system/lib64 缺 libvndksupport.so）
[[service]]  mkdir -p /kmlibs && cp /mnt/system/system/lib64/{libhardware,libvndksupport,libnetutils}.so ... 
             → 写 /kmlibs/ld.config.txt（namespace.default.isolated = false）
             → 写 /kmlibs/km.sh（export LD_CONFIG_FILE + exec）

# gatekeeper 属性
[[service]]  setprop vendor.gatekeeper.disable_spu true; setprop is_security_level_spu 0

# 安全世界回连（带 init 的 notify-topology socket）
[[service]]  /vendor/bin/qseecomd  sockets=["notify-topology"]  ready_property="vendor.sys.listeners.registered"

# 安全服务网关（两个 socket + capabilities）
[[service]]  /vendor/bin/ssgtzd  sockets=["ssgtzd","ssgtzd_opener"]  capabilities=["WAKE_ALARM","BLOCK_SUSPEND"]

# 密钥服务（不能有 gate，也不能晚启动：qseecomd 的 listener 会抢走 keymaster applet）
[[service]]  /kmlibs/km.sh → keymint-service-qti  registers=[3 个附加服务名]

# 凭据服务（AIDL，需 VINTF 描述）
[[service]]  /kmlibs/km.sh → gatekeeper-service-qti  description=/vendor/etc/vintf/manifest_pineapple.xml

# 启动槽位
[[service]]  /vendor/bin/hw/android.hardware.boot-service.qti  ready_service=IBootControl/default

# weaver（放最后，起不来也不挡前面；内联 manifest）
[[service]]  /kmlibs/km.sh → weaver-service.nxp-qti  manifest='''<manifest ...>'''

[firmware] partition = "modem"      # TEE applet 在 modem 分区
[partitions] mounted = ["persist"]  # weaver 文件和 WLAN mac
```

### 7. manet 与 shennong 的差异（不能照抄的部分）

| 项 | shennong | manet |
|---|---|---|
| 触摸 | synaptics_tcm2 | **goodix_core + xiaomi_touch** |
| NFC/eSE | 无 | **stm_nfc_i2c + nxp-nci** |
| display | 1440×3200/560 | 同（可共用） |
| stock | OS3.0.309.0 | **OS3.0.306.0** |

### 8. CI 的坑

- **`if:` 里的 `&&` 必须加引号**（`&` 是 YAML 锚点符号，不加引号整个 workflow 解析失败，job 数为 0）
- **`needs` 里有失败 job 会让其他 job 被 skip**，除非 `if` 加 `always()`
- fork 的 Actions 需要手动启用一次
- **PAT 改 `.github/workflows/` 需要 `workflow` scope**（只有 `repo` 会被拒）
- **拉 Actions 日志需要 admin 权限**（`repo` 权限下 `/logs` 返回 "Must have admin rights"）

### 9. 移植代码时测试失败：先看断言，别删测试

搬 shennong 的 `crates/` 时**只搬了代码，没搬 `devices/shennong.toml`**，于是它自带的两个测试失败：

```
test tests::parses_shennong ... FAILED        lib.rs:748
  assertion failed: the slider is raised so its light reads evenly
test tests::services_use_the_partitions_they_run_from ... FAILED   lib.rs:897
```

原因：

```rust
let cfg = DeviceConfig::parse(include_str!("../../../devices/shennong.toml")).unwrap();
assert_eq!(cfg.display.gamma, 2.2);              // 上游版 shennong.toml 没有 gamma
assert!(listeners.uses(Path::new("/vendor")));   // 上游版没有 sockets 配置
```

**测试本身是对的，是配置没跟上**。把 shennong fork 的 `devices/shennong.toml` 复制过来后，两个测试立刻通过，CI 五个 job 全绿。

**教训**：测试失败时先读它断言什么、读哪份数据，再决定改代码还是改配置。删测试等于掩盖「配置没跟上」的真问题——以后 shennong.toml 再被覆盖就没人发现了。

### 10. Skyward 自身更新的三处配套

要让「设置 → Skyward 更新」真正可用，需要三件事同时成立：

```rust
// ① 指向自己的 fork，否则会去上游找（上游没有 manet 的包）
pub const LATEST: &str = "https://api.github.com/repos/PXIAOHE/skyward/releases/latest";
```

```yaml
# ② release tag 必须等于 zip 名里的版本段：
#    newest() 按 skyward-<tag>-<codename>- 匹配
if [ "$GITHUB_REF_TYPE" = tag ]; then TAG="$GITHUB_REF_NAME"; else TAG="${GITHUB_SHA::7}"; fi
```

```rust
# ③ 下载源顺序：API 的 CDN 直链 → github.com → gh-proxy 镜像
located → self.link → format!("https://gh-proxy.com/{link}")
```

**实测下载速度**（2026-11-10，本地网络）：

| 源 | 速度 |
|---|---|
| github.com 直连 | 1.0~1.6 MB/s |
| gh-proxy.com | 1.03 MB/s |
| ghfast.top | 591 KB/s |
| ghproxy.net | 108 KB/s |
| ghps.cc / 99988866.xyz / moeyy.xyz / gitmirror.com | 失效（403/000） |

直连与 gh-proxy 持平，镜像只作最后兜底——正常路径完全不经过第三方。

## 三、CI 与同步流程（定稿）

### workflow 已改为

```yaml
# 1. 不指定 device 时只编 manet（不再全量 22 台）
if [ -z "$DEVICE" ]; then DEVICE=manet; fi
if [ "$DEVICE" = all ]; then ... else 只编指定机型 fi

# 2. push 到 main 也发 release（不只 tag）
if: "always() && (github.ref_type == 'tag' || github.ref == 'refs/heads/main') && needs.recovery.result == 'success'"

# 3. release tag = main-<sha>，资产匿名可下载
TAG: ${{ github.ref_type == 'tag' && github.ref_name || format('main-{0}', github.sha) }}
```

### 完整自动化流程

```
改代码 → git push（token 需 repo + workflow scope）
       → CI 只编 manet（约 2 分钟）
       → 自动建 release（tag main-<sha>）
       → curl 匿名拉包（不需要用户手动下载）
       → 解压 → dd 到 recovery_b（用户确认一次）
```

**拉包命令**：
```sh
curl -L -o sky.zip "https://github.com/PXIAOHE/skyward/releases/download/main-<full_sha>/skyward-<sha7>-manet-OS3.0.306.0.WNMCNXM.zip"
```

### 同步上游新版本时

1. `git fetch upstream && git merge upstream/main`
2. 冲突重点：`crates/skyward/src/services.rs`（shennong 改动）、`input.rs`（PR #30）、`devices/manet.toml`
3. merge 后**必测**：解密、WiFi、触摸息屏
4. 如果上游改了服务启动逻辑，`sockets` / `manifest` 字段可能又要重新适配

## 四、产物

```
项目/skyward-probe/                         fork 工程（PXIAOHE/skyward）
项目/skyward-probe/devices/manet.toml       设备配置
项目/shennong-fork/                         参考的 shennong 分支（来源）
/storage/emulated/0/Download/skyward-manet-v10-可用版-备份.img   回滚用（0.1.2 houji 二进制版）
/storage/emulated/0/Download/recovery_b_before_skyward.img       更早的 TWRP 备份
```

## 五、遗留

- **U盘不识别**：需要 Skyward 支持 USB host 模式（切 `usb_role` 到 host + 供电 + usb-storage），上游没有这个功能
- **息屏休眠**：0.2.0 有该功能，但 recovery 场景用得少，未验证
- **`Lint and test` 恒失败**：shennong 的测试断言与上游不一致，不影响产物