# 休眠超时的语义分叉与上游合并手册

本分支在休眠管理上**故意与上游不同**，位置只有一处：`rmk/src/sleep.rs` 的
`run_sleep_manager` / `manage_sleep_state`。这是 fork 里唯一一处“行为语义”分叉，
其余差异都是新增模块或 API 适配。本文说明分叉的确切内容、上游改动这块时怎么处理、
以及怎么确认分叉没有在合并中被静默丢掉。

## 一、分叉的确切内容

| 项 | 上游 | 本分支 |
| --- | --- | --- |
| 超时来源 | 编译期常量 `SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS` | 初值取自该常量，运行期存在 `SLEEP_TIMEOUT_SECONDS: AtomicU32` |
| 0 的含义 | 编译期裁剪：整个休眠管理关闭 | 只关空闲超时；显式请求仍然生效 |
| 实现方式 | `if 常量 == 0 { pending().await; return; }` | 状态机内 `timeout_disabled` 分支，继续监听 `TIMEOUT_CHANGED` |
| 运行期改超时 | 不支持 | Vial 通道 `0xF0`，`set_sleep_timeout_seconds` + `TIMEOUT_CHANGED` |
| 从 0 改回非 0 | 不可能（管理器已 `pending` 到底，必须复位） | 立即生效 |

被保留的两条显式请求路径都在 `rmk/src/ble/mod.rs`：

- `ble/mod.rs:469` 广播超时（`BleHostError::BleHost(Error::Timeout)`）→ `request_sleep()`
- `ble/mod.rs:710` HID Control Point 写入 `0`（`HID_CTRL_SUSPEND`）→ `request_sleep()`

保留本地语义的理由，合并争论时按这三条回答即可：

1. 上游的 `0` 是编译期常量语义，本分支的 `0` 是用户随时可改的运行期值。`pending()`
   一旦进入就再也收不到 `TIMEOUT_CHANGED`，改回 180 也不生效 —— 运行期超时功能
   必须以“能从 0 恢复”为前提。
2. 这两条请求是协议层信号，不是用户偏好：吞掉广播超时会让无线键盘在无主机连接下
   保持唤醒，纯耗电；HID Control Point 是主机的明确指令。
3. 调试时需要“永不休眠”的场景走 USB 路径，不经过这个管理器；需要更长空闲时间
   用 Vial 设 3600 即可，不需要一个全关档位。

## 二、合并前：怎么发现上游动了这块

```bash
cd /d/projects/rmk
LAST=$(git log -1 --merges --format=%h HEAD)          # 上次合并提交
git log --oneline $LAST..upstream/main -- rmk/src/sleep.rs rmk/src/keyboard.rs rmk/src/ble/mod.rs
git diff HEAD..upstream/main -- rmk/src/sleep.rs      # 只看这一个文件
git merge-tree --write-tree HEAD upstream/main | grep CONFLICT   # 预演冲突
```

三条判断：

- **列表为空** → 上游没碰，直接合并，跳过本文其余部分。
- **有提交但无冲突** → 最危险的情况，自动合并会挑一侧，必须走第三节的情形 C。
- **有冲突** → 走第四节流程，冲突位置本身就在提醒你。

## 三、五种情形与应对

### A. 只改了注释、日志或无关函数

无操作。合并后跑一次验证清单即可。

### B. 改了 `run_sleep_manager` 的启动位置或签名

上游 2026-11-13 就做过一次（从 `ble` 任务挪到 `Keyboard::run`）。原则不变：
**保留存储加载 + 保留运行期原子量，不要退回常量判断**。

- 启动点变了 → 确认 `Keyboard::run` 之外没有第二处调用（`Signal` 只允许一个消费者）
- 签名叫参数了 → 把参数当作初始值喂给 `SLEEP_TIMEOUT_SECONDS`，不要绕开原子量
- 出现 `if ... == 0 { pending() }` → 删掉，让状态机接住

### C. 重构了 `manage_sleep_state`（重点）

这是唯一可能**编译通过、测试也可能通过，但行为悄悄变了**的地方。合并后必须逐行看
`git diff HEAD -- rmk/src/sleep.rs`，确认四个锚点还在：

1. `timeout_disabled` 分支存在，且 `Either::First(true) => break`（显式请求能睡）
2. `TIMEOUT_CHANGED` 在**两个**分支里都被 `select` 到（能改、能从 0 恢复）
3. 收到 `TIMEOUT_CHANGED` 后同时更新 `idle_timeout` 和 `timeout_disabled`
4. 醒着的 `select` 里，`Timer::after(idle_timeout)` 仍参与竞争

上游如果在重构中把 `select(SLEEP_INPUT.wait(), Timer::after(idle_timeout))` 换成
单个 `SLEEP_INPUT.wait()`（比如改成由别处 tick 驱动），不要接受 —— 那是把本地
运行期超时功能整个抹掉。

### D. 上游自己也引入运行期超时

这时分叉失去意义，**主动放弃本地实现、跟随上游**，前提是它满足：能从 0 恢复、
0 时显式请求仍生效。若不满足，保留本地实现并在 `LOCAL-PATCHES.md` 记一条，
说明上游版本缺哪一点。

### E. 常量改名 / 默认值变化

`SPLIT_CENTRAL_SLEEP_TIMEOUT_SECONDS` 被搬走或改名时，除了改 `use`，注意测试
`zero_timeout_keeps_explicit_sleep_requests` 开头的 `assert_eq!(..., 0)`。

- 默认值改成非 0 后该断言会失败，这是**有意的告警**，不要删断言；
- 正确做法是把测试改成显式 `set_sleep_timeout_seconds(0)` 再断言行为，
  让这条测试不再依赖配置默认值。

## 四、冲突解决流程

1. `git merge upstream/main --no-commit --no-ff`
2. `git diff --name-only --diff-filter=U` 看冲突文件；`sleep.rs` 冲突**优先保留本地状态机**，
   再把上游新增的日志、注释、事件发布往上贴。
3. 冲突解决后立刻 `grep -n "pending::<()>\|TIMEOUT_CHANGED\|timeout_disabled" rmk/src/sleep.rs`，
   对照第五节的判据表。
4. 跑验证清单，通过后再提交。

## 五、判据表：分叉是否还活着

```bash
cd /d/projects/rmk
grep -n "pending::<()>" rmk/src/sleep.rs            # 必须无输出
grep -c "TIMEOUT_CHANGED" rmk/src/sleep.rs          # 期望 ≥ 4
grep -c "timeout_disabled" rmk/src/sleep.rs         # 期望 ≥ 4
grep -n "request_sleep()" rmk/src/ble/mod.rs        # 期望 2 处（469 / 710 附近）
```

| 现象 | 结论 |
| --- | --- |
| `pending::<()>` 出现 | 已退回上游语义，分叉丢失 |
| `TIMEOUT_CHANGED` 计数下降 | 运行期改超时可能失效，查第三节 C |
| `timeout_disabled` 消失 | 0 变成“全关”或“全开”，两种都不对 |
| `ble/mod.rs` 的 `request_sleep()` 少一处 | 显式请求路径被上游改动吞掉 |

## 六、合并后验证清单

```bash
cd /d/projects/rmk/rmk
cargo nextest run --config-file ../.config/nextest.toml \
  --no-default-features --features=split,vial,storage,async_matrix sleep::
```

7 项全过，每项守的东西：

| 测试 | 守住什么 |
| --- | --- |
| `keyboard_runs_configured_sleep_manager` | 管理器由 `Keyboard::run` 启动，不是第二处 |
| `zero_timeout_keeps_explicit_sleep_requests` | 0 只关空闲；显式请求仍睡；活动能唤醒 |
| `sleeps_when_idle_and_wakes_on_activity` | 基本空闲超时与唤醒 |
| `keyboard_activity_restarts_timeout_and_wakes_from_sleep` | 运行期改超时真的生效（靠直接设原子量测） |
| `pointing_activity_restarts_the_idle_timeout` | 指针活动参与计时 |
| `joystick_motion_keeps_awake_but_centered_samples_allow_sleep` | 摇杆居中样本不误判为活动 |
| `sleep_request_skips_the_idle_timeout` | 显式请求不等空闲期 |

另需一并跑 `cargo check` 的 8 组 featureset（见 `.github/ci/_lib.sh` 的
`RMK_TEST_FEATURESETS`），因为 `sleep.rs` 的存储加载带 `_ble` 门控。

**主机测试之外的手工验收**（有硬件时做，没有则记录未验）：
Vial 设 0 → 静置 2 分钟不应睡 → 断开主机触发广播超时 → 应睡 → 按任意键 → 应醒 →
Vial 改 180 → 静置 3 分钟以上 → 应睡。

## 七、历史

- **2026-11-06**：引入运行期超时（Vial `0xF0`），确立“0 只关空闲”语义。
- **2026-11-13**：合入上游 `708c9ee32`。上游把 `ble/sleep.rs` 重命名为顶层
  `sleep.rs`，git 识别为 rename 并把两侧内容合成了一个文件 —— 自动合并“过于顺利”，
  需要人工处理：仓库路径 `crate::ble::sleep` → `crate::sleep`；存储读取补 `_ble` 门控；
  上游两条测试按本地语义调整（`zero_timeout_disables_idle_sleep_and_requests` 改名
  `zero_timeout_keeps_explicit_sleep_requests`；键盘活动那条不再起第二个管理器，
  因为 `Signal` 只允许一个消费者，改为直接设原子量为 1 秒）。
