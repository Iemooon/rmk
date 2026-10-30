# nRF52833 可选 DFU 窗口

仅在 nRF52833 工程中开启 `rmk/nrf52833_dfu_window`。
默认关闭，52820、52840 等工程不要开启。这个开关选择 52833 HAL、存储和
Adafruit 跳转方式，不负责设置 Flash 分区，也不会启动 BLE 无线任务。
可同时用于 BLE 或 Gazell 应用；实际 bootloader 必须支持 GPREGRET `0x57`。

## 开启方法

多芯片工程把开关加在自己的 52833 feature 中，例如：

```toml
[features]
nrf52833 = ["embassy-nrf/nrf52833", "rmk/nrf52833_dfu_window"]
```

在启动 USB/Vial 之前构造唯一的窗口对象，再与应用和存储任务一起运行：

```rust,ignore
use rmk::core_traits::Runnable;

// 必须在启动 USB/Vial 之前调用。
let mut dfu = rmk::dfu_window::DfuWindow::new();
// application 包含现有 USB、键盘和存储任务；这些任务必须并发运行。
rmk::embassy_futures::join::join(application, dfu.run()).await;
```

如果现有工程使用输入处理器注册，也可以把该对象作为 `Runnable` 加入现有任务组。
开启 Cargo feature 只编入实现；必须构造并运行这个对象才会生效。
构造后不能遗漏运行，否则软件 DFU 请求会一直等待清理任务。

迁移时移除该工程对旧 `common/dfu_window.rs`、`common/dfu_policy.rs` 的引用和任务注册，
不能同时运行两套窗口。保留正确的存储任务、分区及 `VialConfig.insecure = true`。
未迁移的工程仍可继续使用原来的 common 实现。

## 行为

- 窗口任务启动约 15 秒时写入 ARMED，第 50 秒清除；持续供电不会自动跳转。
- 在窗口内断电，下次启动读到 ARMED 后，清除并读回确认，再进入 bootloader。
- Vial 等本机软件调用 `boot::jump_to_bootloader()` 时，先停止窗口写标记，
  清除并再次读回确认，最后才跳转。失败时留在应用，可再次请求。
- 使用 RMK 存储队列，预留用户槽位 240，沿用 `WKDFU1:A` / `WKDFU1:C`。
- 未构造窗口的应用继续使用原有软件跳转行为。现有底层准备接口保留兼容性，
  不会仅因启用 `storage` 就自动拦截 Vial 跳转。

窗口起点是任务开始运行，应用应在启动时及时运行它。带电池时只拔 USB 不算断电。
这延续现有持久标记方案，没有新增对看门狗等其他复位原因的严格区分。

## 升级与验证

RMK 的存储校验包含 feature 列表；第一次增删该开关可能清除旧改键和配对。
这也不能解决 bootloader 自身的刷写就绪时间或 USB hub 重新枚举问题。

每个实际工程迁移后仍须检查 Flash/RAM 分区，实测断电窗口、Vial 跳转、重复刷入、
USB 重新识别。库的编译和主机测试不等于实物验收，不会自动烧录。

2026-10-27 历史验证：主机 Vial/BLE 分体测试 636 项通过；52833 开启该 feature 的 ARM
库 debug/release 编译通过；52840 不开启该 feature 的 ARM 库 release 编译通过；
同时选择 52840 HAL 和该 feature 的错误组合被编译器拒绝。

2026-10-30 同步上游 `71556128`：窗口任务及软件跳转准备流程保留，仍默认关闭、仅限 nRF52833。上述历史结果不代表本次同步重新验证；本次检查范围见 LEMON-NMK-FEATURES.md。
