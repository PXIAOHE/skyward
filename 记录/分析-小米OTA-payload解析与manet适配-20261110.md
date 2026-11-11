日期：2026-11-10
类型：分析
状态：验证完成（recovery 提取 + SHA-256 双路一致），未编译
关联：项目/skyward-probe、devices/manet.toml、K70 Pro（manet / sm8650）、OS3.0.306.0.WNMCNXM

# 小米 OTA payload 解析 与 Skyward manet 适配

## 一、Skyward 项目结论

### 是什么

`kirklin/skyward`（`PXIAOHE/skyward` 是其 fork）——Rust 从零写的第三方 recovery。
功能：双系统、解密 data、安装、在线更新、备份恢复、文件管理、终端、MTP、清除数据、替换原厂 recovery。

### 支持状态

| 档位 | 机型 | 能否解密 data |
|---|---|---|
| 正式支持 | nezha（17 Ultra）、ishtar（13 Ultra） | ✅ |
| 🚧 施工中 | 小米 13/14 全系、K80/K80 Pro/K90 系等 19 台 | ❌（配置里没有解密服务） |
| **无** | **manet（K70 Pro）** | 需自行新增配置 |

**关键发现**：「施工中不解密」不是代码能力问题，而是 `devices/*.toml` 里**没写解密所需的 `[[service]]`**。
已支持的 ishtar 配置里有 qseecomd + keymint + gatekeeper 三条服务，并靠 `ready_property` / `ready_service` / `ready_hardware_service` **阻塞等待 HAL 注册**，所以不存在时序猜测问题。

### 构建方式

```sh
cargo ndk -t arm64-v8a --platform 30 build --release -p skyward
cargo build --release -p skyward-pack
cargo ndk -t arm64-v8a --platform 30 build --release -p skyward-system-init --target-dir target/system-init   # 需 RUSTFLAGS=-C target-feature=+crt-static

skyward-pack stock --config devices/manet.toml --out target/stock/manet/recovery.img
skyward-pack recovery --stock ... --binary ... --system-init ... --config devices/manet.toml --font ... --mono ... --out ...
```

`skyward-pack stock` 用 `unpayload` crate **从 URL 流式只取 recovery 分区**（HTTP range），不下载整包。

## 二、小米 OTA payload 结构（实测 K70 Pro 完整包）

包：`manet-ota_full-OS3.0.306.0.WNMCNXM-user-16.0-5d044d135d.zip`
大小：`7241182496` 字节

### zip 结构

```
offset 0     : local file header (META-INF/com/android/metadata, STORED 718 字节)
offset 69    : metadata 数据（含 ota-property-files）
offset 4838  : payload 数据起点（**直接是 CrAU，不是 PK 头**）
```

metadata 内容（关键字段）：

```
ota-type=AB
ota-property-files=payload_metadata.bin:4838:268291,payload.bin:4838:7241173986,payload_properties.txt:7241178882:156,...
post-build=Redmi/manet/manet:16/BP2A.250605.031.A3/16OS3.1.260916...:user/release-key
```

### payload 头

```
offset 0  : "CrAU"            4 字节 magic
offset 4  : version           8 字节大端 = 2
offset 12 : manifest_size     8 字节大端 = 268000
offset 20 : （4 字节额外前缀）
offset 24 : protobuf manifest 开始 ← 实测必须从 24 开始解析才能正确解析
```

### 小米改过的 proto 字段映射（**与 AOSP 不同，必看**）

`InstallOperation`：

| 字段号 | AOSP 定义 | 本机实测 |
|---|---|---|
| 1 | type | type ✅ |
| 2 | src_extents | **data_offset** |
| 3 | src_length | **data_length** |
| 6 | data_sha256_hash | **dst_extents** |
| 8 | data_length | **data_sha256_hash**（32 字节） |

`PartitionUpdate`：`1` = partition_name，`7` = new_partition_info（`1`=size，`2`=sha256），`8` = install_operations（repeated）。

### 两个致命单位陷阱

1. **`data_offset` 是相对 payload 数据区**，不是相对文件头。
   - 数据区起点（zip 内）= `4838 + 24 + 268000 + 4 + 263` = **273129**
   - 实际位置 = `273129 + data_offset`

2. **`dst_extents.start_block` 是块号，不是字节**。
   - 字节偏移 = `start_block * 4096`

### 操作类型

| type | 含义 | 本包出现 |
|---|---|---|
| 0 | REPLACE（未压缩） | 否 |
| 1 | REPLACE_BZ（bzip2） | 36 个（稀疏零区，每块 48~80 字节压缩→2MB 近全零） |
| 6 | ZERO | 否 |
| 8 | REPLACE_XZ（xz） | 14 个（真实数据，每块约 2MB） |

### 提取结果

```
recovery 分区：
  size   = 104857600（100 MB）
  ops    = 50（14 REPLACE_XZ + 36 REPLACE_BZ）
  sha256 = 4a39faaf69631db5d773b296356ec472917e9577c89696e08e94d3369bc92e56
  镜像结构 = boot.img header v4，kernel_size=0，ramdisk 26.8 MB，末尾 AVBf footer
```

**提取脚本逻辑**（已验证 sha256 完全匹配）：

```python
DATA = 273129
for op in ops:
    if op.type == 8:  raw = lzma.decompress(read(DATA+op.off, op.len))
    elif op.type == 1: raw = bz2.decompress(read(DATA+op.off, op.len))
    elif op.type == 0: raw = read(DATA+op.off, op.len)
    else: continue            # ZERO 保持零
    cur = 0
    for start_block, num_blocks in op.extents:
        n = num_blocks * 4096
        buf[start_block*4096 : start_block*4096+n] = raw[cur:cur+n]
        cur += n
```

## 三、manet（K70 Pro）实测硬件参数

| 项目 | 值 | 来源 |
|---|---|---|
| 代号 / 型号 | manet / 23117RK66C | ro.product |
| SoC | sm8650（骁龙 8 Gen 3） | — |
| 屏幕 | 1440×3200 / density 560 | wm size / density |
| 触摸 | **Goodix**（`goodix_core` + `xiaomi_touch` + `goodix_fod`，固件 2BS1000） | /proc/modules |
| WLAN | `qca_cld3_kiwi_v2`（与 K80 相同） | /proc/modules |
| 触感 | `qcom-haptics`，`lra_frequency_hz` = 1720 | /sys/class/qcom-haptics |
| 分区 | A/B + super + metadata，当前槽位 `_b` | /dev/block/by-name |
| USB | /config/usb_gadget/g1、g2 | — |

### 解密链（实机确认全部存在且运行）

| 组件 | 路径 | 注册名 | 就绪信号 |
|---|---|---|---|
| qseecomd | `/vendor/bin/qseecomd` | — | `vendor.sys.listeners.registered` = true ✅ |
| KeyMint | `/vendor/bin/hw/android.hardware.security.keymint-service-qti` | `android.hardware.security.keymint.IKeyMintDevice/default` | 服务注册 |
| Gatekeeper | `/vendor/bin/hw/android.hardware.gatekeeper-service-qti` | `android.hardware.gatekeeper.IGatekeeper/default`（**AIDL**，ishtar 是 HIDL） | 服务注册 |
| Weaver | `/odm/bin/hw/android.hardware.weaver-service.nxp-qti` | `android.hardware.weaver.IWeaver/default` | 服务注册 |

**注意**：weaver 是 **NXP eSE 实现**（不走高通 TEE），依赖 `/dev/nq-nci`（`u:object_r:nfc_device:s0`）+ `nxp_nci`/`stm_nfc_i2c`/`nfc` 三个内核模块 + NFC 固件。
skyward 通过 binder 调 weaver，**不需要自身有 nfc_device 权限**（实测 root 进程都被 SELinux 拒绝访问该节点），权限隔离在服务里。
SELinux：weaver 二进制 `u:object_r:vendor_hal_weaver_default_exec:s0`，用户 `vendor_nxp_weaver`（UID 2911）存在于 `/vendor/etc/passwd`。

## 四、遗留风险

1. **Goodix 触摸未被任何现成配置验证过**（K80 是 focaltech、小米14 系是 synaptics）——最大不确定性，需实机试；失败可用 adb root shell 继续操作
2. **eSE 芯片初始化时序**：`nxp_nci` 加载后 `/dev/nq-nci` 出现的时间，weaver 起太早会启动失败且 recovery 里无 watchdog 兜底
3. EDL 重启命令、自动重刷 recovery 需要实测

## 五、产物

```
项目/skyward-probe/devices/manet.toml      配置文件（含 stock 段与解密四服务）
临时/recovery_stock.img                    原厂 recovery（100MB，sha256 已验证）
临时/ota/manet-ota.zip                     完整 OTA 包（6.7GB）
临时/payload_metadata.bin                  payload manifest（268291 字节）
```